use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct RpcRequest<'a> {
    pub id: u64,
    pub method: &'a str,
    pub params: Value,
}

#[derive(Debug, Deserialize)]
pub struct RpcResponse {
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_json_rpc_id_method_and_params_without_shell_encoding() {
        let encoded = serde_json::to_string(&RpcRequest {
            id: 7,
            method: "thread/list",
            params: serde_json::json!({"searchTerm": "hello; touch /tmp/pwned"}),
        })
        .unwrap();
        let parsed: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(parsed["id"], 7);
        assert_eq!(parsed["method"], "thread/list");
        assert_eq!(parsed["params"]["searchTerm"], "hello; touch /tmp/pwned");
    }
}
