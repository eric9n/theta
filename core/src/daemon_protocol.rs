use serde::{Deserialize, Serialize};

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
    pub fn success<T: Serialize>(val: T) -> Self {
        Self {
            result: Some(serde_json::to_value(val).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            result: None,
            error: Some(msg.into()),
        }
    }
}
