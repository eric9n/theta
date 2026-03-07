use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};

pub const MAX_DAEMON_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_DAEMON_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonRequest {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
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

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            result: None,
            error: Some(msg.into()),
        }
    }
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
