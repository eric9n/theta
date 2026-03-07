use anyhow::{Result, bail};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use tokio::io::{AsyncRead, AsyncReadExt};

pub const MAX_DAEMON_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_DAEMON_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const PROVIDER_RATE_LIMIT_CODES: [i64; 2] = [301606, 301607];

#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonRequest {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonErrorKind {
    BadRequest,
    Timeout,
    Provider,
    RateLimit,
    Transport,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonError {
    pub kind: DaemonErrorKind,
    pub method: Option<String>,
    pub provider_code: Option<i64>,
    pub message: String,
}

impl DaemonError {
    pub fn new(
        kind: DaemonErrorKind,
        message: impl Into<String>,
        method: Option<String>,
        provider_code: Option<i64>,
    ) -> Self {
        Self {
            kind,
            method,
            provider_code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::BadRequest, message, None, None)
    }

    pub fn transport(message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::Transport, message, None, None)
    }

    pub fn timeout(method: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::Timeout, message, Some(method.into()), None)
    }

    pub fn internal(method: Option<String>, message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::Internal, message, method, None)
    }

    pub fn provider(method: impl Into<String>, message: impl Into<String>) -> Self {
        let method = method.into();
        let message = message.into();
        let provider_code = extract_provider_code(&message);
        let kind = if provider_code.is_some_and(is_rate_limit_code) {
            DaemonErrorKind::RateLimit
        } else {
            DaemonErrorKind::Provider
        };

        Self::new(kind, message, Some(method), provider_code)
    }

    pub fn is_rate_limit(&self) -> bool {
        self.kind == DaemonErrorKind::RateLimit
            || self.provider_code.is_some_and(is_rate_limit_code)
    }
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "theta daemon {:?} error", self.kind)?;
        if let Some(method) = &self.method {
            write!(f, " [{}]", method)?;
        }
        if let Some(code) = self.provider_code {
            write!(f, " code={}", code)?;
        }
        write!(f, ": {}", self.message)
    }
}

impl Error for DaemonError {}

#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub result: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_optional_daemon_error")]
    pub error: Option<DaemonError>,
}

impl DaemonResponse {
    pub fn success_value(val: serde_json::Value) -> Self {
        Self {
            result: Some(val),
            error: None,
        }
    }

    pub fn try_success<T: Serialize>(val: T) -> serde_json::Result<Self> {
        Ok(Self::success_value(serde_json::to_value(val)?))
    }

    pub fn error(err: DaemonError) -> Self {
        Self {
            result: None,
            error: Some(err),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DaemonErrorWire {
    Structured(DaemonError),
    Legacy(String),
}

fn deserialize_optional_daemon_error<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<DaemonError>, D::Error>
where
    D: Deserializer<'de>,
{
    let wire = Option::<DaemonErrorWire>::deserialize(deserializer)?;
    Ok(wire.map(|value| match value {
        DaemonErrorWire::Structured(err) => err,
        DaemonErrorWire::Legacy(message) => DaemonError::internal(None, message),
    }))
}

pub fn encode_json_line<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    let mut payload = serde_json::to_vec(value)?;
    payload.push(b'\n');
    Ok(payload)
}

pub async fn read_bounded_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
    frame_name: &str,
) -> Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();

    loop {
        let byte = match reader.read_u8().await {
            Ok(byte) => byte,
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                if buf.is_empty() {
                    return Ok(None);
                }
                bail!("{frame_name} ended before newline terminator");
            }
            Err(err) => return Err(err.into()),
        };

        buf.push(byte);
        if buf.len() > max_bytes {
            bail!("{frame_name} exceeded {max_bytes} bytes");
        }

        if byte == b'\n' {
            return Ok(Some(buf));
        }
    }
}

pub fn extract_provider_code(message: &str) -> Option<i64> {
    PROVIDER_RATE_LIMIT_CODES
        .into_iter()
        .find(|code| message.contains(&code.to_string()))
}

pub fn is_rate_limit_code(code: i64) -> bool {
    PROVIDER_RATE_LIMIT_CODES.contains(&code)
}

pub fn is_transient_quote_rate_limit_message(message: &str) -> bool {
    extract_provider_code(message).is_some_and(is_rate_limit_code)
        || message.contains("Request rate limit")
        || message.contains("Too many option securities request within one minute")
}

pub fn is_transient_quote_rate_limit_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<DaemonError>()
        .is_some_and(DaemonError::is_rate_limit)
        || is_transient_quote_rate_limit_message(&err.to_string())
}

pub fn is_provider_code(err: &anyhow::Error, code: i64) -> bool {
    err.downcast_ref::<DaemonError>()
        .and_then(|daemon_err| daemon_err.provider_code)
        == Some(code)
        || err.to_string().contains(&code.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        DaemonError, DaemonErrorKind, DaemonResponse, is_provider_code,
        is_transient_quote_rate_limit_error,
    };
    use anyhow::anyhow;

    #[test]
    fn deserializes_legacy_string_errors() {
        let response: DaemonResponse = serde_json::from_value(serde_json::json!({
            "result": null,
            "error": "legacy error"
        }))
        .expect("response deserializes");

        let err = response.error.expect("error present");
        assert_eq!(err.kind, DaemonErrorKind::Internal);
        assert_eq!(err.message, "legacy error");
    }

    #[test]
    fn structured_rate_limit_errors_are_detectable() {
        let err = anyhow!(DaemonError::provider(
            "option_quote",
            "response error: 301606 Request rate limit",
        ));

        assert!(is_transient_quote_rate_limit_error(&err));
        assert!(is_provider_code(&err, 301606));
    }
}
