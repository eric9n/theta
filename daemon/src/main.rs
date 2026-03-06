use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tracing::{info, warn, error};
use longport::quote::QuoteContext;
use theta::daemon_protocol::{DaemonRequest, DaemonResponse};
use serde_json::json;

const SOCKET_PATH: &str = "/tmp/theta.sock";
const SDK_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Manages the lifecycle and health of the LongPort SDK QuoteContext.
struct SdkManager {
    ctx: RwLock<Option<Arc<QuoteContext>>>,
    config: Arc<longport::Config>,
}

impl SdkManager {
    async fn new() -> Result<Self> {
        let config = Arc::new(longport::Config::from_env().context("missing SDK env vars")?.dont_print_quote_packages());
        Ok(Self {
            ctx: RwLock::new(None),
            config,
        })
    }

    /// Provides a thread-safe context, initializing it if necessary.
    async fn get_ctx(&self) -> Result<Arc<QuoteContext>> {
        // Optimistic read
        {
            let read = self.ctx.read().await;
            if let Some(ctx) = read.as_ref() {
                return Ok(Arc::clone(ctx));
            }
        }

        // Initialize or recover
        let mut write = self.ctx.write().await;
        // Double check after acquiring write lock
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
            // Probe with a minimal request (e.g. trading days for US market)
            let probe = timeout(Duration::from_secs(5), ctx.trading_days(longport::Market::US, time::OffsetDateTime::now_utc().date(), time::OffsetDateTime::now_utc().date())).await;
            if probe.is_err() || probe.unwrap().is_err() {
                warn!("SDK health check failed. Reconnect scheduled.");
                self.invalidate_ctx().await;
            }
        } else {
            // If none, try to init
            let _ = self.get_ctx().await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Required for rustls 0.23+ 
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt::init();
    info!("Starting Theta Resilient SDK Proxy Daemon...");

    let sdk_manager = Arc::new(SdkManager::new().await?);

    // Initial attempt to connect (non-blocking for daemon start)
    {
        let mgr = Arc::clone(&sdk_manager);
        tokio::spawn(async move {
            if let Err(e) = mgr.get_ctx().await {
                warn!("Initial SDK connection failed: {}. Will retry on demand.", e);
            }
        });
    }

    // Health check loop
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

    if std::path::Path::new(SOCKET_PATH).exists() {
        std::fs::remove_file(SOCKET_PATH).context("failed to remove existing socket")?;
    }

    let listener = UnixListener::bind(SOCKET_PATH).context("failed to bind to unix socket")?;
    
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o666))?;

    info!("Raw SDK Gateway listening on {}", SOCKET_PATH);

    use tokio::signal::unix::{signal, SignalKind};
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

    // Cleanup
    if std::path::Path::new(SOCKET_PATH).exists() {
        let _ = std::fs::remove_file(SOCKET_PATH);
        info!("Cleaned up socket at {}", SOCKET_PATH);
    }

    Ok(())
}

async fn handle_connection(mut stream: UnixStream, mgr: Arc<SdkManager>) -> Result<()> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 { break; }

        let req: DaemonRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
                Err(e) => {
                let _ = writer.write_all((serde_json::to_string(&DaemonResponse::error(e.to_string()))? + "\n").as_bytes()).await;
                continue;
            }
        };

        // Get context OR attempt reconnection
        let response = match mgr.get_ctx().await {
            Ok(ctx) => {
                // Truly generic SDK dispatch with timeout protection
                match timeout(SDK_TIMEOUT, dispatch_raw_sdk_call(&req, &ctx)).await {
                    Ok(Ok(val)) => DaemonResponse::success(val),
                    Ok(Err(e)) => {
                        error!("SDK Proxy functional failure [{}]: {}", req.method, e);
                        DaemonResponse::error(e.to_string())
                    }
                    Err(_) => {
                        error!("SDK Proxy call timed out [{}]", req.method);
                        mgr.invalidate_ctx().await; // Invalidate on suspected hang
                        DaemonResponse::error("SDK operation timed out".to_string())
                    }
                }
            }
            Err(e) => {
                error!("SDK Proxy unavailable: {}", e);
                DaemonResponse::error(format!("SDK unavailable: {}", e))
            }
        };

        let resp_json = serde_json::to_string(&response)? + "\n";
        writer.write_all(resp_json.as_bytes()).await?;
    }
    Ok(())
}

async fn dispatch_raw_sdk_call(req: &DaemonRequest, ctx: &QuoteContext) -> Result<serde_json::Value> {
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
            struct Params { symbol: String, expiry: String }
            let p: Params = serde_json::from_value(req.params.clone())?;
            let format = time::format_description::parse("[year]-[month]-[day]")?;
            let expiry = time::Date::parse(&p.expiry, &format)?;
            Ok(json!(ctx.option_chain_info_by_date(p.symbol, expiry).await?))
        }
        "calc_indexes" => {
            #[derive(serde::Deserialize)]
            struct Params { symbols: Vec<String>, indexes: Vec<String> }
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
            struct Params { market: longport::Market, start: String, end: String }
            let p: Params = serde_json::from_value(req.params.clone())?;
            let format = time::format_description::parse("[year]-[month]-[day]")?;
            let start = time::Date::parse(&p.start, &format)?;
            let end = time::Date::parse(&p.end, &format)?;
            Ok(json!(ctx.trading_days(p.market, start, end).await?))
        }
        _ => anyhow::bail!("Method not found in raw SDK proxy: {}", req.method),
    }
}
