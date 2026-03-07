use anyhow::{Context, Result, bail};
use longport::quote::QuoteContext;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use theta::daemon_protocol::{
    DaemonRequest, DaemonResponse, MAX_DAEMON_REQUEST_BYTES, encode_json_line, read_bounded_frame,
};
use theta::runtime::theta_socket_path;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};
use tracing::{error, info, warn};

const SDK_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60);
const SOCKET_CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const RUNTIME_DIR_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;

/// Manages the lifecycle and health of the LongPort SDK QuoteContext.
struct SdkManager {
    ctx: RwLock<Option<Arc<QuoteContext>>>,
    config: Arc<longport::Config>,
}

impl SdkManager {
    async fn new() -> Result<Self> {
        let config = Arc::new(
            longport::Config::from_env()
                .context("missing SDK env vars")?
                .dont_print_quote_packages(),
        );
        Ok(Self {
            ctx: RwLock::new(None),
            config,
        })
    }

    /// Provides a thread-safe context, initializing it if necessary.
    async fn get_ctx(&self) -> Result<Arc<QuoteContext>> {
        {
            let read = self.ctx.read().await;
            if let Some(ctx) = read.as_ref() {
                return Ok(Arc::clone(ctx));
            }
        }

        let mut write = self.ctx.write().await;
        if let Some(ctx) = write.as_ref() {
            return Ok(Arc::clone(ctx));
        }

        info!("Initializing LongPort SDK connection...");
        match timeout(SDK_TIMEOUT, QuoteContext::try_new(Arc::clone(&self.config))).await {
            Ok(Ok((ctx, _))) => {
                let ctx = Arc::new(ctx);
                *write = Some(Arc::clone(&ctx));
                info!("LongPort SDK connection established.");
                Ok(ctx)
            }
            Ok(Err(e)) => {
                error!("SDK handshake failed: {}", e);
                anyhow::bail!("SDK handshake failed: {}", e);
            }
            Err(_) => {
                error!("SDK initialization timed out");
                anyhow::bail!("SDK initialization timed out");
            }
        }
    }

    async fn invalidate_ctx(&self) {
        let mut write = self.ctx.write().await;
        *write = None;
        warn!("SDK context invalidated due to health or error.");
    }

    async fn health_check(&self) {
        let ctx_opt = { self.ctx.read().await.clone() };
        if let Some(ctx) = ctx_opt {
            let today = time::OffsetDateTime::now_utc().date();
            match timeout(
                Duration::from_secs(5),
                ctx.trading_days(longport::Market::US, today, today),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    warn!("SDK health check trading_days failed: {}", e);
                    self.invalidate_ctx().await;
                }
                Err(_) => {
                    warn!("SDK health check trading_days timed out");
                    self.invalidate_ctx().await;
                }
            }
        } else if let Err(e) = self.get_ctx().await {
            warn!("SDK health check could not establish context: {}", e);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt::init();
    info!("Starting Theta Resilient SDK Proxy Daemon...");

    let socket_path = theta_socket_path()?;
    ensure_runtime_dir(&socket_path)?;
    prepare_socket_path(&socket_path).await?;

    let sdk_manager = Arc::new(SdkManager::new().await?);

    {
        let mgr = Arc::clone(&sdk_manager);
        tokio::spawn(async move {
            if let Err(e) = mgr.get_ctx().await {
                warn!(
                    "Initial SDK connection failed: {}. Will retry on demand.",
                    e
                );
            }
        });
    }

    {
        let mgr = Arc::clone(&sdk_manager);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(HEALTH_CHECK_INTERVAL);
            loop {
                interval.tick().await;
                mgr.health_check().await;
            }
        });
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind to unix socket {}", socket_path.display()))?;
    set_private_permissions(&socket_path, SOCKET_MODE)?;

    info!("Raw SDK Gateway listening on {}", socket_path.display());

    use tokio::signal::unix::{SignalKind, signal};
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    loop {
        tokio::select! {
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((stream, _)) => {
                        let mgr = Arc::clone(&sdk_manager);
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, mgr).await {
                                warn!("Proxy connection closed with error: {}", e);
                            }
                        });
                    }
                    Err(e) => error!("Accept error: {}", e),
                }
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, shutting down...");
                break;
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down...");
                break;
            }
        }
    }

    cleanup_socket_file(&socket_path);
    Ok(())
}

fn ensure_runtime_dir(socket_path: &Path) -> Result<()> {
    let runtime_dir = socket_path
        .parent()
        .context("theta socket path is missing a parent directory")?;
    std::fs::create_dir_all(runtime_dir).with_context(|| {
        format!(
            "failed to create theta runtime directory {}",
            runtime_dir.display()
        )
    })?;
    set_private_permissions(runtime_dir, RUNTIME_DIR_MODE)
}

async fn prepare_socket_path(socket_path: &Path) -> Result<()> {
    if !socket_path.exists() {
        return Ok(());
    }

    let metadata = std::fs::symlink_metadata(socket_path).with_context(|| {
        format!(
            "failed to inspect existing socket path {}",
            socket_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if !metadata.file_type().is_socket() {
            bail!(
                "refusing to remove non-socket file at {}",
                socket_path.display()
            );
        }
    }

    match timeout(SOCKET_CONNECT_TIMEOUT, UnixStream::connect(socket_path)).await {
        Ok(Ok(_)) => bail!("theta-daemon already running at {}", socket_path.display()),
        Ok(Err(_)) | Err(_) => {
            std::fs::remove_file(socket_path).with_context(|| {
                format!("failed to remove stale socket {}", socket_path.display())
            })?;
            info!("Removed stale socket at {}", socket_path.display());
            Ok(())
        }
    }
}

fn set_private_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

fn cleanup_socket_file(socket_path: &Path) {
    if socket_path.exists() {
        if let Err(err) = std::fs::remove_file(socket_path) {
            warn!(
                "Failed to clean up socket {}: {}",
                socket_path.display(),
                err
            );
        } else {
            info!("Cleaned up socket at {}", socket_path.display());
        }
    }
}

fn parse_request_frame(frame: &[u8]) -> std::result::Result<DaemonRequest, DaemonResponse> {
    serde_json::from_slice(frame)
        .map_err(|e| DaemonResponse::error(format!("Invalid daemon request: {}", e)))
}

async fn handle_connection(stream: UnixStream, mgr: Arc<SdkManager>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    loop {
        let frame = match read_bounded_frame(
            &mut reader,
            MAX_DAEMON_REQUEST_BYTES,
            "daemon request frame",
        )
        .await
        {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(err) => {
                let payload = encode_json_line(&DaemonResponse::error(err.to_string()))?;
                let _ = writer.write_all(&payload).await;
                break;
            }
        };

        let req = match parse_request_frame(&frame) {
            Ok(req) => req,
            Err(response) => {
                let payload = encode_json_line(&response)?;
                let _ = writer.write_all(&payload).await;
                continue;
            }
        };

        let response = match mgr.get_ctx().await {
            Ok(ctx) => match timeout(SDK_TIMEOUT, dispatch_raw_sdk_call(&req, &ctx)).await {
                Ok(Ok(val)) => DaemonResponse::success_value(val),
                Ok(Err(e)) => {
                    error!("SDK Proxy functional failure [{}]: {}", req.method, e);
                    DaemonResponse::error(e.to_string())
                }
                Err(_) => {
                    error!("SDK Proxy call timed out [{}]", req.method);
                    mgr.invalidate_ctx().await;
                    DaemonResponse::error("SDK operation timed out".to_string())
                }
            },
            Err(e) => {
                error!("SDK Proxy unavailable: {}", e);
                DaemonResponse::error(format!("SDK unavailable: {}", e))
            }
        };

        let resp_json = encode_json_line(&response)?;
        writer.write_all(&resp_json).await?;
    }
    Ok(())
}

async fn dispatch_raw_sdk_call(
    req: &DaemonRequest,
    ctx: &QuoteContext,
) -> Result<serde_json::Value> {
    match req.method.as_str() {
        "quote" => {
            let symbols: Vec<String> = serde_json::from_value(req.params.clone())?;
            Ok(json!(ctx.quote(symbols).await?))
        }
        "option_quote" => {
            let symbols: Vec<String> = serde_json::from_value(req.params.clone())?;
            Ok(json!(ctx.option_quote(symbols).await?))
        }
        "option_chain_expiry_date_list" => {
            let symbol: String = serde_json::from_value(req.params.clone())?;
            Ok(json!(ctx.option_chain_expiry_date_list(symbol).await?))
        }
        "option_chain_info_by_date" => {
            #[derive(serde::Deserialize)]
            struct Params {
                symbol: String,
                expiry: String,
            }
            let p: Params = serde_json::from_value(req.params.clone())?;
            let format = time::format_description::parse("[year]-[month]-[day]")?;
            let expiry = time::Date::parse(&p.expiry, &format)?;
            Ok(json!(
                ctx.option_chain_info_by_date(p.symbol, expiry).await?
            ))
        }
        "calc_indexes" => {
            #[derive(serde::Deserialize)]
            struct Params {
                symbols: Vec<String>,
                indexes: Vec<String>,
            }
            let p: Params = serde_json::from_value(req.params.clone())?;

            let mut sdk_indexes = Vec::new();
            for s in p.indexes {
                let idx = match s.as_str() {
                    "LastDone" => longport::quote::CalcIndex::LastDone,
                    "ImpliedVolatility" => longport::quote::CalcIndex::ImpliedVolatility,
                    "Delta" => longport::quote::CalcIndex::Delta,
                    "Gamma" => longport::quote::CalcIndex::Gamma,
                    "Theta" => longport::quote::CalcIndex::Theta,
                    "Vega" => longport::quote::CalcIndex::Vega,
                    "Rho" => longport::quote::CalcIndex::Rho,
                    _ => continue,
                };
                sdk_indexes.push(idx);
            }
            Ok(json!(ctx.calc_indexes(p.symbols, sdk_indexes).await?))
        }
        "trading_days" => {
            #[derive(serde::Deserialize)]
            struct Params {
                market: longport::Market,
                start: String,
                end: String,
            }
            let p: Params = serde_json::from_value(req.params.clone())?;
            let format = time::format_description::parse("[year]-[month]-[day]")?;
            let start = time::Date::parse(&p.start, &format)?;
            let end = time::Date::parse(&p.end, &format)?;
            Ok(json!(ctx.trading_days(p.market, start, end).await?))
        }
        _ => anyhow::bail!("Method not found in raw SDK proxy: {}", req.method),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::io::{AsyncWriteExt, duplex};

    #[test]
    fn ensure_runtime_dir_sets_private_permissions() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("nested").join("theta.sock");
        ensure_runtime_dir(&socket_path).expect("runtime dir");

        let runtime_dir = socket_path.parent().expect("parent");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(runtime_dir)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, RUNTIME_DIR_MODE);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prepare_socket_path_refuses_live_socket() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("theta.sock");
        let _listener = UnixListener::bind(&socket_path).expect("bind live socket");

        let err = prepare_socket_path(&socket_path)
            .await
            .expect_err("live socket should fail");
        assert!(err.to_string().contains("already running"));
        assert!(socket_path.exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prepare_socket_path_removes_stale_socket() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("theta.sock");
        {
            let _listener = UnixListener::bind(&socket_path).expect("bind socket");
        }

        assert!(socket_path.exists());
        prepare_socket_path(&socket_path)
            .await
            .expect("stale socket should be removed");
        assert!(!socket_path.exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prepare_socket_path_rejects_non_socket_files() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("theta.sock");
        std::fs::write(&socket_path, b"not a socket").expect("write file");

        let err = prepare_socket_path(&socket_path)
            .await
            .expect_err("non-socket file should fail");
        assert!(err.to_string().contains("non-socket"));
    }

    #[test]
    fn parse_request_frame_returns_error_response_for_invalid_json() {
        let err = parse_request_frame(br#"{not-json}\n"#).expect_err("should fail");
        assert!(
            err.error
                .expect("error message")
                .contains("Invalid daemon request")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_request_frame_rejects_oversized_payload() {
        let (mut writer, mut reader) = duplex(MAX_DAEMON_REQUEST_BYTES + 32);
        tokio::spawn(async move {
            let payload = vec![b'x'; MAX_DAEMON_REQUEST_BYTES + 1];
            writer.write_all(&payload).await.expect("write payload");
        });

        let err = read_bounded_frame(
            &mut reader,
            MAX_DAEMON_REQUEST_BYTES,
            "daemon request frame",
        )
        .await
        .expect_err("oversized frame should fail");
        assert!(err.to_string().contains("exceeded"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn socket_file_permissions_are_private() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("theta.sock");
        let _listener = UnixListener::bind(&socket_path).expect("bind socket");
        set_private_permissions(&socket_path, SOCKET_MODE).expect("permissions");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&socket_path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, SOCKET_MODE);
        }
    }

    #[test]
    fn default_socket_path_is_not_tmp() {
        let dir = tempdir().expect("tempdir");
        unsafe {
            std::env::remove_var(theta::runtime::THETA_SOCKET_PATH_ENV);
            std::env::set_var("HOME", dir.path());
        }

        let path = theta_socket_path().expect("socket path");
        assert_eq!(
            path,
            PathBuf::from(dir.path()).join(".theta/run/theta.sock")
        );
        assert!(!path.starts_with("/tmp/theta.sock"));
    }
}
