use std::path::PathBuf;

use anyhow::anyhow;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::FmtSpan;

pub struct Guard {
    _tracing_appender_guards: Vec<WorkerGuard>,
}

pub async fn init() -> anyhow::Result<Guard> {
    use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Layer};

    let dir = PathBuf::from("log");
    #[allow(clippy::used_underscore_binding)]
    let _tracing_appender_guards = {
        let hostname = PathBuf::from(hostname().await?);
        let filename_error = hostname.join("error.log");
        let filename_info = hostname.join("info.log");
        let (writer_file_error, guard_error) = tracing_appender::non_blocking(
            tracing_appender::rolling::daily(&dir, filename_error),
        );
        let (writer_file_info, guard_info) = tracing_appender::non_blocking(
            tracing_appender::rolling::daily(&dir, filename_info),
        );
        let layer_file_error = fmt::Layer::new()
            .with_writer(writer_file_error)
            .with_ansi(true)
            .with_file(false)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_filter(
                EnvFilter::from_default_env()
                    .add_directive(tracing::Level::ERROR.into()),
            );
        let layer_file_info = fmt::Layer::new()
            .with_writer(writer_file_info)
            .with_ansi(true)
            .with_file(false)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_filter(
                EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
            );
        let subscriber = tracing_subscriber::registry()
            .with(layer_file_error)
            .with(layer_file_info);
        tracing::subscriber::set_global_default(subscriber)?;
        vec![guard_error, guard_info]
    };

    let guard = Guard {
        _tracing_appender_guards,
    };
    Ok(guard)
}

async fn hostname() -> anyhow::Result<String> {
    // TODO Consider a cross-platofrm way to lookup hostname.
    let bytes = cmd("hostname", &[]).await?;
    let str = String::from_utf8(bytes)?;
    let str = str.trim();
    Ok(str.to_string())
}

async fn cmd(cmd: &str, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(anyhow!("Failure in '{} {:?}'. out: {:?}", cmd, args, out))
    }
}
