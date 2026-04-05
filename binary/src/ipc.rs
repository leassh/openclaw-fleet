use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: u64,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn success(id: u64, value: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(value),
            error: None,
            id,
        }
    }

    pub fn error(id: u64, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError { code, message }),
            id,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct IpcMessage {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl IpcMessage {
    pub fn event(event_type: String, data: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: "event".to_string(),
            params: serde_json::json!({
                "type": event_type,
                "data": data,
            }),
        }
    }
}
