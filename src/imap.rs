use std::{result, sync::Arc};

use async_imap::types::Capability;
use futures::{Stream, StreamExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use crate::cfg;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Bye")]
    Bye,

    #[error("TimedOut")]
    TimedOut,

    #[error("BadFolderPath: {0}")]
    BadFolderPath(String),

    #[error("MsgInvalidMissingBody uid={uid}")]
    FetchInvalidMissingBody { uid: u32 },

    #[error("FetchInvalidMissingHeaders uid={uid}")]
    FetchInvalidMissingHeaders { uid: u32 },

    #[error("FetchInvalidMissingUid")]
    FetchInvalidMissingUid,

    #[error("Idle event channel hung-up")]
    IdleEventChannelHungUp,

    #[error("MailParse: {0:?}")]
    MailParse(#[from] mailparse::MailParseError),

    #[error("Imap error: {0:?}")]
    Imap(#[from] async_imap::error::Error),

    #[error("IO error: {0:?}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = result::Result<T, Error>;

type ImapSession = async_imap::Session<TlsStream<TcpStream>>;
type Meta = async_imap::types::Mailbox;

pub struct Session {
    session: ImapSession,
}

impl Session {
    pub async fn close(&mut self) -> Result<()> {
        self.session.close().await?;
        Ok(())
    }

    pub async fn new(account: &cfg::ImapAccount) -> Result<Self> {
        let mut session = connect(account).await?;
        let capabilities = session.capabilities().await?;
        tracing::info!(
            ?account,
            capabilities = ?capabilities.iter().collect::<Vec<&Capability>>(),
            "New IMAP session."
        );
        Ok(Self { session })
    }

    pub async fn examine(&mut self, mailbox: &str) -> Result<Meta> {
        let meta = self.session.examine(mailbox).await.map_err(|error| {
            tracing::error!(?error, "Failed to examine mailbox.");
            error
        })?;
        tracing::info!(?mailbox, exists = meta.exists, "Switched mailbox.");
        Ok(meta)
    }

    pub async fn list_mailboxes(
        &mut self,
    ) -> Result<impl Stream<Item = String> + '_> {
        let reference_name = None; // None is equivalent to Some("")
        let mailbox_pattern = Some("*");
        let names =
            self.session.list(reference_name, mailbox_pattern).await?;
        let names = names.filter_map(|result| async {
            if let Err(error) = &result {
                // TODO Should we terminate the stream of keep going/trying?
                tracing::error!(?error, "Failed name.");
            }
            result.ok().map(|name| name.name().to_string())
        });
        Ok(names.boxed())
    }

    #[tracing::instrument(skip_all)]
    pub async fn fetch_msgs<'a>(
        &'a mut self,
        mailbox: &'a str,
    ) -> Result<(Meta, impl Stream<Item = (u32, Vec<u8>)> + 'a)> {
        tracing::debug!(?mailbox, "Fetching all messages.");
        let meta: Meta = self.examine(mailbox).await?;
        let fetches = self.session.fetch("1:*", "(RFC822 UID)").await?;
        let msgs = fetches.filter_map(move |result| async {
            let mailbox = mailbox.to_string();
            if let Err(error) = &result {
                // TODO Should we terminate the stream of keep going/trying?
                tracing::error!(?mailbox, ?error, "Failed fetch.");
            }
            result.ok().and_then(|f| {
                f.uid.and_then(move |uid| {
                    f.body().map(|body| (uid, body.to_vec()))
                })
            })
        });
        Ok((meta, msgs.boxed()))
    }
}

#[tracing::instrument(skip_all)]
async fn connect(
    cfg::ImapAccount {
        addr,
        port,
        user,
        pass,
    }: &cfg::ImapAccount,
) -> Result<ImapSession> {
    let tcp = TcpStream::connect((addr.as_str(), *port)).await?;
    let tls = tls_stream(addr, tcp).await?;
    let client = async_imap::Client::new(tls);
    let session: ImapSession =
        client.login(user, pass).await.map_err(|(e, _)| e)?;
    Ok(session)
}

async fn tls_stream(
    domain: &str,
    tcp_stream: TcpStream,
) -> std::io::Result<TlsStream<TcpStream>> {
    let mut root_cert_store = rustls::RootCertStore::empty();
    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth(); // I guess this was previously the default?
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

    let domain = rustls_pki_types::ServerName::try_from(domain)
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid domain: {:?}", domain),
            )
        })?
        .to_owned();

    let tls_stream = connector.connect(domain, tcp_stream).await?;
    Ok(tls_stream)
}
