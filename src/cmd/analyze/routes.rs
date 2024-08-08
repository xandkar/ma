use std::collections::HashMap;

use futures::StreamExt;
use mail_parser::{Host, Received};

use crate::{
    cfg::Cfg,
    data::{self, Msg},
};

pub async fn trace(reduce: bool, cfg: &Cfg) -> anyhow::Result<()> {
    let db = data::Storage::connect(&cfg.db).await?;
    let mut msgs = db.fetch_messages();
    let mut routes = HashMap::new();
    let mut max: usize = 0;
    while let Some(Ok(Msg { hash: _, raw })) = msgs.next().await {
        if let Some(msg) =
            mail_parser::MessageParser::new().parse_headers(&raw[..])
        {
            for part in msg.parts {
                for received in part
                    .headers
                    .into_iter()
                    .filter_map(|h| h.value.into_received())
                {
                    let Received {
                        from,
                        by,

                        from_ip: _,
                        from_iprev: _,
                        for_: _,
                        with: _,
                        tls_version: _,
                        tls_cipher: _,
                        id: _,
                        ident: _,
                        helo: _,
                        helo_cmd: _,
                        via: _,
                        date: _,
                    } = received;
                    let dst = host_class(by, reduce);
                    let src = host_class(from, reduce);
                    routes
                        .entry((src, dst))
                        .and_modify(|count: &mut usize| {
                            *count = count.saturating_add(1);
                            if *count > max {
                                max = *count;
                            }
                        })
                        .or_insert(1);
                }
            }
        }
    }
    println!("strict digraph G {{");
    for ((src, dst), count) in &routes {
        if *count > 1 {
            let penwidth = penwidth(*count, max);
            println!("    {src:?} -> {dst:?} [penwidth={penwidth}]");
        }
    }
    println!("}}");
    Ok(())
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_sign_loss)]
fn penwidth(cur: usize, tot: usize) -> u8 {
    let min: f64 = 1.0;
    let max: f64 = 10.0;
    let ratio: f64 = cur as f64 / tot as f64;
    (min + ratio * (max - min)).floor() as u8
}

fn host_class(host: Option<Host>, reduce: bool) -> String {
    match host {
        None => "unknown".to_string(),
        Some(Host::Name(name)) => {
            let name = name.trim().to_lowercase();
            let parts: Vec<&str> = name.split('.').collect::<Vec<&str>>();
            match &parts[..] {
                [] => "name:empty".to_string(),
                [_] => "name:private".to_string(),
                [.., a, b] if reduce => format!("name:pub={}.{}", a, b),
                [..] => format!("name:pub={}", name),
            }
        }
        Some(Host::IpAddr(std::net::IpAddr::V4(addr))) => {
            if reduce {
                let [o1, ..] = addr.octets();
                format!("ipv4={}.x.x.x", o1)
            } else {
                format!("ipv4={}", addr)
            }
        }
        Some(Host::IpAddr(std::net::IpAddr::V6(addr))) => {
            if reduce {
                let [o1, ..] = addr.octets();
                format!("ipv6={}:x:x:x:x:x:x:x:x:x:x:x:x:x:x:x", o1)
            } else {
                format!("ipv6={}", addr)
            }
        }
    }
}
