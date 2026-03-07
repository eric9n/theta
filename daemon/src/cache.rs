use crate::backend::QuoteBackend;
use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::Date;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, warn};

const STOCK_QUOTE_TTL: Duration = Duration::from_secs(5);
const OPTION_QUOTE_TTL: Duration = Duration::from_secs(8);
const OPTION_EXPIRY_LIST_TTL: Duration = Duration::from_secs(15 * 60);
const OPTION_CHAIN_INFO_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST: usize = 500;
const OPTION_QUOTE_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct CacheEntry<T> {
    value: T,
    inserted_at: Duration,
}

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> Duration;
}

#[derive(Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
    }
}

pub struct QuoteCache<B: QuoteBackend, C: Clock = SystemClock> {
    backend: Arc<B>,
    clock: C,
    quote_cache: RwLock<HashMap<String, CacheEntry<Value>>>,
    option_quote_cache: RwLock<HashMap<String, CacheEntry<Value>>>,
    expiry_cache: RwLock<HashMap<String, CacheEntry<Vec<String>>>>,
    chain_info_cache: RwLock<HashMap<String, CacheEntry<Vec<Value>>>>,
    option_quote_rate_limit_until: RwLock<Option<Duration>>,
    quote_fetch_lock: Mutex<()>,
    option_quote_fetch_lock: Mutex<()>,
    expiry_fetch_lock: Mutex<()>,
    chain_info_fetch_lock: Mutex<()>,
}

impl<B: QuoteBackend> QuoteCache<B, SystemClock> {
    pub fn new(backend: Arc<B>) -> Self {
        Self::with_clock(backend, SystemClock)
    }
}

impl<B: QuoteBackend, C: Clock> QuoteCache<B, C> {
    pub fn with_clock(backend: Arc<B>, clock: C) -> Self {
        Self {
            backend,
            clock,
            quote_cache: RwLock::new(HashMap::new()),
            option_quote_cache: RwLock::new(HashMap::new()),
            expiry_cache: RwLock::new(HashMap::new()),
            chain_info_cache: RwLock::new(HashMap::new()),
            option_quote_rate_limit_until: RwLock::new(None),
            quote_fetch_lock: Mutex::new(()),
            option_quote_fetch_lock: Mutex::new(()),
            expiry_fetch_lock: Mutex::new(()),
            chain_info_fetch_lock: Mutex::new(()),
        }
    }

    pub async fn quote(&self, requested_symbols: Vec<String>) -> Result<Value> {
        self.resolve_cached_batch(
            "quote",
            &requested_symbols,
            &self.quote_cache,
            &self.quote_fetch_lock,
            STOCK_QUOTE_TTL,
            |symbols| self.backend.quote(symbols),
        )
        .await
    }

    pub async fn option_quote(&self, requested_symbols: Vec<String>) -> Result<Value> {
        let requested_keys = normalize_requested_symbols(&requested_symbols);
        if requested_keys.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }

        let unique_keys = dedup_normalized_keys(&requested_keys);
        let now = self.clock.now();
        let mut cached_rows = self
            .read_cached_rows(
                &self.option_quote_cache,
                &unique_keys,
                OPTION_QUOTE_TTL,
                now,
            )
            .await;
        let mut misses = unique_keys
            .iter()
            .filter(|key| !cached_rows.contains_key((*key).as_str()))
            .cloned()
            .collect::<Vec<_>>();

        if !misses.is_empty() {
            let _guard = self.option_quote_fetch_lock.lock().await;

            cached_rows.extend(
                self.read_cached_rows(
                    &self.option_quote_cache,
                    &misses,
                    OPTION_QUOTE_TTL,
                    self.clock.now(),
                )
                .await,
            );
            misses.retain(|key| !cached_rows.contains_key(key));

            if !misses.is_empty() {
                self.ensure_option_quote_cooldown_elapsed(misses.len())
                    .await?;

                for chunk in misses.chunks(MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST) {
                    match self.backend.option_quote(chunk.to_vec()).await {
                        Ok(rows) => {
                            self.clear_option_quote_cooldown().await;
                            let fresh_rows = index_rows_by_symbol(rows);
                            if !fresh_rows.is_empty() {
                                let inserted_at = self.clock.now();
                                let mut write = self.option_quote_cache.write().await;
                                for (key, value) in &fresh_rows {
                                    write.insert(
                                        key.clone(),
                                        CacheEntry {
                                            value: value.clone(),
                                            inserted_at,
                                        },
                                    );
                                }
                                cached_rows.extend(fresh_rows);
                            }
                        }
                        Err(err) => {
                            let err_text = err.to_string();
                            if is_rate_limit_error_text(&err_text) {
                                self.activate_option_quote_cooldown().await;
                                warn!(
                                    method = "option_quote",
                                    misses = misses.len(),
                                    requested = requested_keys.len(),
                                    chunk_size = chunk.len(),
                                    cooldown_seconds = OPTION_QUOTE_RATE_LIMIT_COOLDOWN.as_secs(),
                                    "LongPort option quote rate limit activated cooldown: {}",
                                    err_text
                                );
                            } else {
                                warn!(
                                    method = "option_quote",
                                    misses = misses.len(),
                                    requested = requested_keys.len(),
                                    chunk_size = chunk.len(),
                                    "LongPort backend call failed: {}",
                                    err_text
                                );
                            }

                            let unresolved = requested_keys
                                .iter()
                                .filter(|key| !cached_rows.contains_key((*key).as_str()))
                                .count();
                            let cache_hits = unique_keys
                                .iter()
                                .filter(|key| cached_rows.contains_key((*key).as_str()))
                                .count();
                            debug!(
                                method = "option_quote",
                                requested_count = requested_keys.len(),
                                unique_requested_count = unique_keys.len(),
                                cache_hit_count = cache_hits,
                                cache_miss_count = misses.len(),
                                unresolved_count = unresolved,
                                backend_called = true,
                                ttl_seconds = OPTION_QUOTE_TTL.as_secs(),
                                cooldown_active = true,
                                "Daemon cache lookup completed"
                            );
                            return Err(err);
                        }
                    }
                }
            }
        }

        let unresolved = requested_keys
            .iter()
            .filter(|key| !cached_rows.contains_key((*key).as_str()))
            .count();
        let cache_hits = unique_keys
            .iter()
            .filter(|key| cached_rows.contains_key((*key).as_str()))
            .count()
            .saturating_sub(misses.len().saturating_sub(unresolved));

        let cooldown_active = self.is_option_quote_cooldown_active(self.clock.now()).await;

        debug!(
            method = "option_quote",
            requested_count = requested_keys.len(),
            unique_requested_count = unique_keys.len(),
            cache_hit_count = cache_hits,
            cache_miss_count = misses.len(),
            unresolved_count = unresolved,
            backend_called = !misses.is_empty(),
            ttl_seconds = OPTION_QUOTE_TTL.as_secs(),
            cooldown_active = cooldown_active,
            "Daemon cache lookup completed"
        );

        Ok(Value::Array(
            requested_keys
                .iter()
                .filter_map(|key| cached_rows.get(key).cloned())
                .collect(),
        ))
    }

    pub async fn option_chain_expiry_date_list(&self, symbol: String) -> Result<Value> {
        let key = normalize_symbol_key(&symbol);
        let now = self.clock.now();

        if let Some(value) = self
            .read_cached_single(&self.expiry_cache, &key, OPTION_EXPIRY_LIST_TTL, now)
            .await
        {
            log_single_cache_event(
                "option_chain_expiry_date_list",
                1,
                1,
                0,
                false,
                OPTION_EXPIRY_LIST_TTL,
            );
            return Ok(Value::Array(value.into_iter().map(Value::String).collect()));
        }

        let _guard = self.expiry_fetch_lock.lock().await;

        if let Some(value) = self
            .read_cached_single(
                &self.expiry_cache,
                &key,
                OPTION_EXPIRY_LIST_TTL,
                self.clock.now(),
            )
            .await
        {
            log_single_cache_event(
                "option_chain_expiry_date_list",
                1,
                1,
                0,
                false,
                OPTION_EXPIRY_LIST_TTL,
            );
            return Ok(Value::Array(value.into_iter().map(Value::String).collect()));
        }

        let value = self
            .backend
            .option_chain_expiry_date_list(key.clone())
            .await
            .map_err(|err| {
                warn!(
                    method = "option_chain_expiry_date_list",
                    misses = 1,
                    symbol = %key,
                    "LongPort backend call failed: {}",
                    err
                );
                err
            })?;

        self.expiry_cache.write().await.insert(
            key,
            CacheEntry {
                value: value.clone(),
                inserted_at: self.clock.now(),
            },
        );
        log_single_cache_event(
            "option_chain_expiry_date_list",
            1,
            0,
            1,
            true,
            OPTION_EXPIRY_LIST_TTL,
        );
        Ok(Value::Array(value.into_iter().map(Value::String).collect()))
    }

    pub async fn option_chain_info_by_date(&self, symbol: String, expiry: Date) -> Result<Value> {
        let key = chain_info_key(&symbol, expiry);
        let now = self.clock.now();

        if let Some(value) = self
            .read_cached_single(&self.chain_info_cache, &key, OPTION_CHAIN_INFO_TTL, now)
            .await
        {
            log_single_cache_event(
                "option_chain_info_by_date",
                1,
                1,
                0,
                false,
                OPTION_CHAIN_INFO_TTL,
            );
            return Ok(Value::Array(value));
        }

        let _guard = self.chain_info_fetch_lock.lock().await;

        if let Some(value) = self
            .read_cached_single(
                &self.chain_info_cache,
                &key,
                OPTION_CHAIN_INFO_TTL,
                self.clock.now(),
            )
            .await
        {
            log_single_cache_event(
                "option_chain_info_by_date",
                1,
                1,
                0,
                false,
                OPTION_CHAIN_INFO_TTL,
            );
            return Ok(Value::Array(value));
        }

        let value = self
            .backend
            .option_chain_info_by_date(normalize_symbol_key(&symbol), expiry)
            .await
            .map_err(|err| {
                warn!(
                    method = "option_chain_info_by_date",
                    misses = 1,
                    symbol = %normalize_symbol_key(&symbol),
                    expiry = %expiry,
                    "LongPort backend call failed: {}",
                    err
                );
                err
            })?;

        self.chain_info_cache.write().await.insert(
            key,
            CacheEntry {
                value: value.clone(),
                inserted_at: self.clock.now(),
            },
        );
        log_single_cache_event(
            "option_chain_info_by_date",
            1,
            0,
            1,
            true,
            OPTION_CHAIN_INFO_TTL,
        );
        Ok(Value::Array(value))
    }

    async fn resolve_cached_batch<F, Fut>(
        &self,
        method: &'static str,
        requested_symbols: &[String],
        cache: &RwLock<HashMap<String, CacheEntry<Value>>>,
        fetch_lock: &Mutex<()>,
        ttl: Duration,
        fetcher: F,
    ) -> Result<Value>
    where
        F: Fn(Vec<String>) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<Value>>>,
    {
        self.resolve_cached_batch_inner(
            method,
            requested_symbols,
            cache,
            fetch_lock,
            ttl,
            None,
            fetcher,
        )
        .await
    }

    async fn resolve_cached_batch_inner<F, Fut>(
        &self,
        method: &'static str,
        requested_symbols: &[String],
        cache: &RwLock<HashMap<String, CacheEntry<Value>>>,
        fetch_lock: &Mutex<()>,
        ttl: Duration,
        chunk_size: Option<usize>,
        fetcher: F,
    ) -> Result<Value>
    where
        F: Fn(Vec<String>) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<Value>>>,
    {
        let requested_keys = normalize_requested_symbols(requested_symbols);
        if requested_keys.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }

        let unique_keys = dedup_normalized_keys(&requested_keys);
        let now = self.clock.now();
        let mut cached_rows = self.read_cached_rows(cache, &unique_keys, ttl, now).await;
        let mut misses = unique_keys
            .iter()
            .filter(|key| !cached_rows.contains_key((*key).as_str()))
            .cloned()
            .collect::<Vec<_>>();

        if !misses.is_empty() {
            let _guard = fetch_lock.lock().await;

            cached_rows.extend(
                self.read_cached_rows(cache, &misses, ttl, self.clock.now())
                    .await,
            );
            misses.retain(|key| !cached_rows.contains_key(key));

            if !misses.is_empty() {
                let backend_rows = if let Some(chunk_size) = chunk_size {
                    self.fetch_in_chunks(&misses, chunk_size, &fetcher).await
                } else {
                    fetcher(misses.clone()).await
                }
                .map_err(|err| {
                    warn!(
                        method = method,
                        misses = misses.len(),
                        requested = requested_keys.len(),
                        "LongPort backend call failed: {}",
                        err
                    );
                    err
                })?;

                let fresh_rows = index_rows_by_symbol(backend_rows);
                if !fresh_rows.is_empty() {
                    let inserted_at = self.clock.now();
                    let mut write = cache.write().await;
                    for (key, value) in &fresh_rows {
                        write.insert(
                            key.clone(),
                            CacheEntry {
                                value: value.clone(),
                                inserted_at,
                            },
                        );
                    }
                }
                cached_rows.extend(fresh_rows);
            }
        }

        let unresolved = requested_keys
            .iter()
            .filter(|key| !cached_rows.contains_key((*key).as_str()))
            .count();
        let cache_hits = unique_keys
            .iter()
            .filter(|key| cached_rows.contains_key((*key).as_str()))
            .count()
            .saturating_sub(misses.len().saturating_sub(unresolved));

        debug!(
            method = method,
            requested_count = requested_keys.len(),
            unique_requested_count = unique_keys.len(),
            cache_hit_count = cache_hits,
            cache_miss_count = misses.len(),
            unresolved_count = unresolved,
            backend_called = !misses.is_empty(),
            ttl_seconds = ttl.as_secs(),
            "Daemon cache lookup completed"
        );

        Ok(Value::Array(
            requested_keys
                .iter()
                .filter_map(|key| cached_rows.get(key).cloned())
                .collect(),
        ))
    }

    async fn fetch_in_chunks<F, Fut>(
        &self,
        misses: &[String],
        chunk_size: usize,
        fetcher: &F,
    ) -> Result<Vec<Value>>
    where
        F: Fn(Vec<String>) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<Value>>>,
    {
        let mut rows = Vec::new();
        for chunk in misses.chunks(chunk_size) {
            let mut fetched = fetcher(chunk.to_vec()).await?;
            rows.append(&mut fetched);
        }
        Ok(rows)
    }

    async fn read_cached_single<T: Clone>(
        &self,
        cache: &RwLock<HashMap<String, CacheEntry<T>>>,
        key: &str,
        ttl: Duration,
        now: Duration,
    ) -> Option<T> {
        let read = cache.read().await;
        read.get(key)
            .filter(|entry| is_fresh(entry.inserted_at, ttl, now))
            .map(|entry| entry.value.clone())
    }

    async fn read_cached_rows(
        &self,
        cache: &RwLock<HashMap<String, CacheEntry<Value>>>,
        keys: &[String],
        ttl: Duration,
        now: Duration,
    ) -> HashMap<String, Value> {
        let read = cache.read().await;
        let mut rows = HashMap::new();
        for key in keys {
            if let Some(entry) = read.get(key)
                && is_fresh(entry.inserted_at, ttl, now)
            {
                rows.insert(key.clone(), entry.value.clone());
            }
        }
        rows
    }

    async fn ensure_option_quote_cooldown_elapsed(&self, misses: usize) -> Result<()> {
        let Some(until) = *self.option_quote_rate_limit_until.read().await else {
            return Ok(());
        };
        let now = self.clock.now();
        if now >= until {
            return Ok(());
        }
        let remaining = until.saturating_sub(now).as_secs();
        warn!(
            method = "option_quote",
            misses = misses,
            cooldown_seconds_remaining = remaining,
            "Skipping upstream option_quote call while local rate-limit cooldown is active"
        );
        anyhow::bail!(
            "local option_quote cooldown active for {}s after upstream rate limit",
            remaining
        );
    }

    async fn activate_option_quote_cooldown(&self) {
        *self.option_quote_rate_limit_until.write().await =
            Some(self.clock.now() + OPTION_QUOTE_RATE_LIMIT_COOLDOWN);
    }

    async fn clear_option_quote_cooldown(&self) {
        *self.option_quote_rate_limit_until.write().await = None;
    }

    async fn is_option_quote_cooldown_active(&self, now: Duration) -> bool {
        self.option_quote_rate_limit_until
            .read()
            .await
            .is_some_and(|until| now < until)
    }
}

fn log_single_cache_event(
    method: &'static str,
    requested_count: usize,
    cache_hit_count: usize,
    cache_miss_count: usize,
    backend_called: bool,
    ttl: Duration,
) {
    debug!(
        method = method,
        requested_count = requested_count,
        unique_requested_count = requested_count,
        cache_hit_count = cache_hit_count,
        cache_miss_count = cache_miss_count,
        unresolved_count = 0,
        backend_called = backend_called,
        ttl_seconds = ttl.as_secs(),
        "Daemon cache lookup completed"
    );
}

fn is_fresh(inserted_at: Duration, ttl: Duration, now: Duration) -> bool {
    now.checked_sub(inserted_at)
        .is_some_and(|elapsed| elapsed <= ttl)
}

fn normalize_requested_symbols(requested_symbols: &[String]) -> Vec<String> {
    requested_symbols
        .iter()
        .map(|symbol| normalize_symbol_key(symbol))
        .collect()
}

fn dedup_normalized_keys(keys: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for key in keys {
        if seen.insert(key.as_str()) {
            deduped.push(key.clone());
        }
    }
    deduped
}

fn index_rows_by_symbol(rows: Vec<Value>) -> HashMap<String, Value> {
    let mut indexed = HashMap::new();
    for row in rows {
        let Some(key) = row
            .get("symbol")
            .and_then(Value::as_str)
            .map(normalize_symbol_key)
        else {
            continue;
        };
        indexed.insert(key, row);
    }
    indexed
}

fn chain_info_key(symbol: &str, expiry: Date) -> String {
    format!("{}|{}", normalize_symbol_key(symbol), expiry)
}

fn normalize_symbol_key(symbol: &str) -> String {
    symbol.trim().to_ascii_uppercase()
}

fn is_rate_limit_error_text(text: &str) -> bool {
    text.contains("301606")
        || text.contains("301607")
        || text.contains("Request rate limit")
        || text.contains("Too many option securities request within one minute")
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use tokio::sync::Notify;

    #[derive(Default)]
    struct MockClock {
        now_millis: AtomicU64,
    }

    impl MockClock {
        fn advance(&self, delta: Duration) {
            self.now_millis
                .fetch_add(delta.as_millis() as u64, Ordering::SeqCst);
        }
    }

    impl Clock for Arc<MockClock> {
        fn now(&self) -> Duration {
            Duration::from_millis(self.now_millis.load(Ordering::SeqCst))
        }
    }

    #[derive(Default)]
    struct FakeBackend {
        quote_calls: AtomicUsize,
        option_quote_calls: AtomicUsize,
        expiry_calls: AtomicUsize,
        chain_info_calls: AtomicUsize,
        quote_rows: HashMap<String, Value>,
        option_quote_rows: HashMap<String, Value>,
        expiry_rows: HashMap<String, Vec<String>>,
        chain_info_rows: HashMap<String, Vec<Value>>,
        quote_fail_once: AtomicUsize,
        option_quote_fail_once: AtomicUsize,
        option_quote_waiters: AtomicUsize,
        option_quote_started: Notify,
        option_quote_release: Notify,
    }

    impl FakeBackend {
        fn with_quote_rows(rows: &[(&str, f64)]) -> Self {
            let mut backend = Self::default();
            backend.quote_rows = rows
                .iter()
                .map(|(symbol, price)| {
                    (
                        normalize_symbol_key(symbol),
                        serde_json::json!({
                            "symbol": normalize_symbol_key(symbol),
                            "last_done": price.to_string(),
                        }),
                    )
                })
                .collect();
            backend
        }

        fn with_option_quote_rows(rows: &[&str]) -> Self {
            let mut backend = Self::default();
            backend.option_quote_rows = rows
                .iter()
                .map(|symbol| {
                    (
                        normalize_symbol_key(symbol),
                        serde_json::json!({
                            "symbol": normalize_symbol_key(symbol),
                            "last_done": "1.0",
                        }),
                    )
                })
                .collect();
            backend
        }
    }

    #[async_trait]
    impl QuoteBackend for FakeBackend {
        async fn quote(&self, symbols: Vec<String>) -> Result<Vec<Value>> {
            self.quote_calls.fetch_add(1, Ordering::SeqCst);
            if self.quote_fail_once.swap(0, Ordering::SeqCst) == 1 {
                anyhow::bail!("quote failed");
            }
            Ok(symbols
                .into_iter()
                .filter_map(|symbol| self.quote_rows.get(&normalize_symbol_key(&symbol)).cloned())
                .collect())
        }

        async fn option_quote(&self, symbols: Vec<String>) -> Result<Vec<Value>> {
            self.option_quote_calls.fetch_add(1, Ordering::SeqCst);
            if self.option_quote_fail_once.swap(0, Ordering::SeqCst) == 1 {
                anyhow::bail!(
                    "response error: 7: detail:Some(WsResponseErrorDetail {{ code: 301607, msg: \"Too many option securities request within one minute\" }})"
                );
            }
            if self.option_quote_waiters.load(Ordering::SeqCst) > 0 {
                self.option_quote_started.notify_waiters();
                self.option_quote_release.notified().await;
            }
            Ok(symbols
                .into_iter()
                .filter_map(|symbol| {
                    self.option_quote_rows
                        .get(&normalize_symbol_key(&symbol))
                        .cloned()
                })
                .collect())
        }

        async fn option_chain_expiry_date_list(&self, symbol: String) -> Result<Vec<String>> {
            self.expiry_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .expiry_rows
                .get(&normalize_symbol_key(&symbol))
                .cloned()
                .unwrap_or_default())
        }

        async fn option_chain_info_by_date(
            &self,
            symbol: String,
            expiry: Date,
        ) -> Result<Vec<Value>> {
            self.chain_info_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .chain_info_rows
                .get(&chain_info_key(&symbol, expiry))
                .cloned()
                .unwrap_or_default())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reuses_stock_quote_cache_within_ttl() {
        let clock = Arc::new(MockClock::default());
        let backend = Arc::new(FakeBackend::with_quote_rows(&[("TSLA.US", 100.0)]));
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        let first = cache.quote(vec!["TSLA.US".to_string()]).await.unwrap();
        let second = cache.quote(vec!["TSLA.US".to_string()]).await.unwrap();

        assert_eq!(first, second);
        assert_eq!(backend.quote_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn only_fetches_overlapping_quote_misses() {
        let clock = Arc::new(MockClock::default());
        let backend = Arc::new(FakeBackend::with_quote_rows(&[
            ("TSLA.US", 100.0),
            ("QQQ.US", 200.0),
        ]));
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        cache
            .quote(vec!["TSLA.US".to_string(), "QQQ.US".to_string()])
            .await
            .unwrap();
        cache.quote(vec!["QQQ.US".to_string()]).await.unwrap();

        assert_eq!(backend.quote_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn option_quote_fetches_in_chunks_and_reuses_cache() {
        let clock = Arc::new(MockClock::default());
        let symbols = (0..(MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST + 25))
            .map(|idx| format!("TSLA{:03}.US", idx))
            .collect::<Vec<_>>();
        let backend = Arc::new(FakeBackend::with_option_quote_rows(
            &symbols.iter().map(String::as_str).collect::<Vec<_>>(),
        ));
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        cache.option_quote(symbols.clone()).await.unwrap();
        cache
            .option_quote(vec![symbols[0].clone(), symbols[1].clone()])
            .await
            .unwrap();

        assert_eq!(backend.option_quote_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn caches_option_expiry_lists() {
        let clock = Arc::new(MockClock::default());
        let mut backend = FakeBackend::default();
        backend.expiry_rows.insert(
            normalize_symbol_key("TSLA.US"),
            vec!["2026-03-20".to_string(), "2026-04-17".to_string()],
        );
        let backend = Arc::new(backend);
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        cache
            .option_chain_expiry_date_list("TSLA.US".to_string())
            .await
            .unwrap();
        cache
            .option_chain_expiry_date_list("tsla.us".to_string())
            .await
            .unwrap();

        assert_eq!(backend.expiry_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn caches_chain_info_by_date() {
        let clock = Arc::new(MockClock::default());
        let expiry = time::macros::date!(2026 - 03 - 20);
        let mut backend = FakeBackend::default();
        backend.chain_info_rows.insert(
            chain_info_key("TSLA.US", expiry),
            vec![serde_json::json!({"symbol":"TSLA.US","price":"400"})],
        );
        let backend = Arc::new(backend);
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        cache
            .option_chain_info_by_date("TSLA.US".to_string(), expiry)
            .await
            .unwrap();
        cache
            .option_chain_info_by_date("tsla.us".to_string(), expiry)
            .await
            .unwrap();

        assert_eq!(backend.chain_info_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ttl_expiry_forces_refetch() {
        let clock = Arc::new(MockClock::default());
        let backend = Arc::new(FakeBackend::with_quote_rows(&[("TSLA.US", 100.0)]));
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        cache.quote(vec!["TSLA.US".to_string()]).await.unwrap();
        clock.advance(STOCK_QUOTE_TTL + Duration::from_secs(1));
        cache.quote(vec!["TSLA.US".to_string()]).await.unwrap();

        assert_eq!(backend.quote_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failures_are_not_cached() {
        let clock = Arc::new(MockClock::default());
        let backend = Arc::new(FakeBackend::with_quote_rows(&[("TSLA.US", 100.0)]));
        backend.quote_fail_once.store(1, Ordering::SeqCst);
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        assert!(cache.quote(vec!["TSLA.US".to_string()]).await.is_err());
        assert!(cache.quote(vec!["TSLA.US".to_string()]).await.is_ok());

        assert_eq!(backend.quote_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn partial_option_quote_responses_only_cache_returned_rows() {
        let clock = Arc::new(MockClock::default());
        let backend = Arc::new(FakeBackend::with_option_quote_rows(&["TSLAA.US"]));
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        let result = cache
            .option_quote(vec!["TSLAA.US".to_string(), "TSLAB.US".to_string()])
            .await
            .unwrap();

        assert_eq!(result.as_array().unwrap().len(), 1);
        assert_eq!(backend.option_quote_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn single_flight_prevents_duplicate_option_quote_fetches() {
        let clock = Arc::new(MockClock::default());
        let backend = Arc::new(FakeBackend::with_option_quote_rows(&["TSLAA.US"]));
        backend.option_quote_waiters.store(1, Ordering::SeqCst);
        let cache = Arc::new(QuoteCache::with_clock(backend.clone(), clock.clone()));

        let first_cache = Arc::clone(&cache);
        let second_cache = Arc::clone(&cache);
        let first = tokio::spawn(async move {
            first_cache
                .option_quote(vec!["TSLAA.US".to_string()])
                .await
                .unwrap()
        });
        backend.option_quote_started.notified().await;
        let second = tokio::spawn(async move {
            second_cache
                .option_quote(vec!["TSLAA.US".to_string()])
                .await
                .unwrap()
        });

        backend.option_quote_release.notify_waiters();
        let _ = tokio::join!(first, second);

        assert_eq!(backend.option_quote_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn option_quote_rate_limit_triggers_local_cooldown() {
        let clock = Arc::new(MockClock::default());
        let backend = Arc::new(FakeBackend::with_option_quote_rows(&["TSLAA.US"]));
        backend.option_quote_fail_once.store(1, Ordering::SeqCst);
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        let first = cache.option_quote(vec!["TSLAA.US".to_string()]).await;
        let second = cache.option_quote(vec!["TSLAA.US".to_string()]).await;

        assert!(first.is_err());
        assert!(
            second
                .unwrap_err()
                .to_string()
                .contains("local option_quote cooldown active")
        );
        assert_eq!(backend.option_quote_calls.load(Ordering::SeqCst), 1);

        clock.advance(OPTION_QUOTE_RATE_LIMIT_COOLDOWN + Duration::from_secs(1));
        cache
            .option_quote(vec!["TSLAA.US".to_string()])
            .await
            .unwrap();
        assert_eq!(backend.option_quote_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn option_quote_preserves_successful_chunks_before_rate_limit_failure() {
        let clock = Arc::new(MockClock::default());
        let symbols = (0..(MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST + 1))
            .map(|idx| format!("TSLA{:03}.US", idx))
            .collect::<Vec<_>>();
        let backend = FakeBackend::with_option_quote_rows(
            &symbols.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let backend = Arc::new(backend);
        let cache = QuoteCache::with_clock(backend.clone(), clock.clone());

        let first_batch = symbols[..MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST].to_vec();
        cache.option_quote(first_batch.clone()).await.unwrap();

        backend.option_quote_fail_once.store(1, Ordering::SeqCst);
        let result = cache.option_quote(symbols.clone()).await;
        assert!(result.is_err());

        let cached_only = cache.option_quote(first_batch.clone()).await.unwrap();
        assert_eq!(cached_only.as_array().unwrap().len(), first_batch.len());
    }
}
