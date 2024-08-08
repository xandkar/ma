use std::collections::{HashMap, HashSet};

use futures::StreamExt;
use mail_parser::{Addr, Address, Group, HeaderName, HeaderValue, Message};

use crate::{
    cfg::Cfg,
    data::{self, Msg},
};

#[tracing::instrument(name = "contacts", skip_all)]
pub async fn analyze(
    cfg: &Cfg,
    noise_threshold: usize,
) -> anyhow::Result<()> {
    let db = data::Storage::connect(&cfg.db).await?;
    let mut msgs = db.fetch_messages();

    // TODO Normalize names.
    // TODO graph_normal2seen
    let mut graph_name2addrs: HashMap<String, HashSet<String>> =
        HashMap::new();
    let mut graph_addr2names: HashMap<String, HashSet<String>> =
        HashMap::new();
    let mut count_name2addr: HashMap<(&str, &str), usize> = HashMap::new();
    let mut count_addr2name: HashMap<(&str, &str), usize> = HashMap::new();

    while let Some(Ok(Msg { hash: _, raw })) = msgs.next().await {
        if let Some(msg) =
            mail_parser::MessageParser::new().parse_headers(&raw[..])
        {
            for (name, addr) in msg_sender_addresses(msg) {
                graph_name2addrs
                    .entry(name.clone())
                    .and_modify(|addrs: &mut HashSet<String>| {
                        addrs.insert(addr.clone());
                    })
                    .or_default();
                graph_addr2names
                    .entry(addr.clone())
                    .and_modify(|names: &mut HashSet<String>| {
                        names.insert(name.clone());
                    })
                    .or_default();
            }
        }
    }
    for (name, addrs) in &graph_name2addrs {
        for addr in addrs {
            count_name2addr
                .entry((name.as_str(), addr.as_str()))
                .and_modify(|count| {
                    *count = count.saturating_add(1);
                })
                .or_insert(1);
            count_addr2name
                .entry((addr.as_str(), name.as_str()))
                .and_modify(|count| {
                    *count = count.saturating_add(1);
                })
                .or_insert(1);
        }
    }
    for (addr, names) in &graph_addr2names {
        for name in names {
            count_name2addr
                .entry((name.as_str(), addr.as_str()))
                .and_modify(|count| {
                    *count = count.saturating_add(1);
                })
                .or_insert(1);
            count_addr2name
                .entry((addr.as_str(), name.as_str()))
                .and_modify(|count| {
                    *count = count.saturating_add(1);
                })
                .or_insert(1);
        }
    }

    let mut names_to_addrs: Vec<(&String, &HashSet<String>)> =
        graph_name2addrs.iter().collect();
    names_to_addrs.sort_by_key(|(name, _)| *name);

    let indent = "\t";

    for (name_0, addrs) in names_to_addrs {
        println!("name {name_0:?}");
        for addr in addrs {
            if let Some(names) = graph_addr2names.get(addr) {
                // How noisy is this addr? Is it just something like "notifications@github.com"?
                let is_noisy = names.len() >= noise_threshold;
                if !is_noisy {
                    let count = count_name2addr
                        .get(&(name_0.as_str(), addr.as_str()))
                        .unwrap_or_else(|| unreachable!(""));
                    println!("{indent}addr ({count}) {addr:?}");
                    for name_i in names {
                        // TODO Meassure edit distance.
                        if name_i != name_0 {
                            let count = count_name2addr
                                .get(&(name_i.as_str(), addr.as_str()))
                                .unwrap_or_else(|| unreachable!(""));
                            println!(
                                "{indent}{indent}name ({count}) {name_i:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    let mut addrs_to_names: Vec<(&String, &HashSet<String>)> =
        graph_addr2names.iter().collect();
    addrs_to_names.sort_by_key(|(name, _)| *name);

    for (addr_0, names) in addrs_to_names {
        println!("addr {addr_0:?}");
        for name in names {
            if let Some(addrs) = graph_name2addrs.get(name) {
                let is_noisy = addrs.len() >= noise_threshold;
                if !is_noisy {
                    let count = count_addr2name
                        .get(&(addr_0.as_str(), name.as_str()))
                        .unwrap_or_else(|| unreachable!(""));
                    println!("{indent}name ({count}) {name:?}");
                    for addr_i in addrs {
                        // TODO Meassure edit distance.
                        if addr_i != addr_0 {
                            let count = count_addr2name
                                .get(&(addr_i.as_str(), name.as_str()))
                                .unwrap_or_else(|| unreachable!(""));
                            println!(
                                "{indent}{indent}addr ({count}) {addr_i:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn msg_sender_addresses(msg: Message) -> Vec<(String, String)> {
    msg.parts
        .into_iter()
        .flat_map(|part| {
            part.headers
                .into_iter()
                .filter_map(|h| match (h.name, h.value) {
                    (
                        HeaderName::From,
                        HeaderValue::Address(Address::List(addresses)),
                    ) => {
                        let addresses: Vec<(String, String)> = addresses
                            .into_iter()
                            .filter_map(|addr| match addr {
                                Addr {
                                    name: Some(name),
                                    address: Some(addr),
                                } => {
                                    Some((name.to_string(), addr.to_string()))
                                }
                                _ => None,
                            })
                            .collect();
                        Some(addresses.into_iter())
                    }
                    (
                        HeaderName::From,
                        HeaderValue::Address(Address::Group(groups)),
                    ) => {
                        let addresses: Vec<(String, String)> = groups
                            .into_iter()
                            .flat_map(|Group { name: _, addresses }| {
                                addresses.into_iter()
                            })
                            .filter_map(|addr| match addr {
                                Addr {
                                    name: Some(name),
                                    address: Some(addr),
                                } => {
                                    Some((name.to_string(), addr.to_string()))
                                }
                                _ => None,
                            })
                            .collect();
                        Some(addresses.into_iter())
                    }
                    _ => None,
                })
                .flatten()
        })
        .collect()
}
