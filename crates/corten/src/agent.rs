//! Agent tools over the Model Context Protocol (feature `agent`): one `POST` route that speaks
//! JSON-RPC 2.0 as MCP's streamable HTTP transport defines it, answering with `application/json`.
//!
//! Tools are the app's actions with a name, a description and a JSON Schema for their input.
//! A tool handler gets the request headers (to authenticate the agent the same way any other
//! client is authenticated: a bearer token, never ambient cookies) and the arguments.
//!
//! ```ignore
//! let tools = Tools::new("card-market", env!("CARGO_PKG_VERSION"))
//!     .tool("inventory.adjust-item", "Change the quantity of an inventory item", schema, |headers, args| async move {
//!         let user = auth::bearer(&headers)?;           // ToolError::Unauthorized when missing
//!         let input: Adjust = serde_json::from_value(args)?;
//!         Ok(json!({ "quantity": inventory::adjust(user, input).await? }))
//!     });
//! let app = router.route("/mcp", tools.route());
//! ```
//!
//! A screen's data is already available to agents through [`crate::Req::screen`]; tools cover
//! the writes. Results go back as `structuredContent` and as the same JSON in a text block, for
//! clients that only read text.

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use axum::{
    body::Bytes,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{post, MethodRouter},
};
use serde_json::{json, Value};

/// Protocol revisions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];

/// Why a tool call failed.
#[derive(Debug)]
pub enum ToolError {
    /// The arguments are wrong; the message goes back to the agent so it can correct them.
    Invalid(String),
    /// No valid credentials: the response is HTTP 401 so the client can authenticate.
    Unauthorized,
    /// Authenticated but not allowed; the message goes back to the agent.
    Forbidden(String),
    /// Anything else. Logged; the agent only sees "internal error".
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

/// Validation errors go back to the agent as one message per field, so it can correct them.
impl From<crate::Errors> for ToolError {
    fn from(errors: crate::Errors) -> Self {
        ToolError::Invalid(errors.to_string())
    }
}

impl From<serde_json::Error> for ToolError {
    fn from(error: serde_json::Error) -> Self {
        ToolError::Invalid(error.to_string())
    }
}

type BoxFuture = Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send>>;
type Handler = Arc<dyn Fn(HeaderMap, Value) -> BoxFuture + Send + Sync>;

struct Tool {
    description: String,
    input_schema: Value,
    read_only: bool,
    handler: Handler,
}

/// The tools one MCP endpoint offers.
#[derive(Clone)]
pub struct Tools {
    name: String,
    version: String,
    tools: Arc<BTreeMap<String, Tool>>,
}

impl Tools {
    pub fn new(name: &str, version: &str) -> Self {
        Tools { name: name.into(), version: version.into(), tools: Arc::default() }
    }

    /// Adds a tool that changes data.
    pub fn tool<F, Fut>(self, name: &str, description: &str, input_schema: Value, handler: F) -> Self
    where
        F: Fn(HeaderMap, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        self.add(name, description, input_schema, false, handler)
    }

    /// Adds a tool that only reads (marked `readOnlyHint`, so clients may call it without asking).
    pub fn read_tool<F, Fut>(self, name: &str, description: &str, input_schema: Value, handler: F) -> Self
    where
        F: Fn(HeaderMap, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        self.add(name, description, input_schema, true, handler)
    }

    fn add<F, Fut>(mut self, name: &str, description: &str, input_schema: Value, read_only: bool, handler: F) -> Self
    where
        F: Fn(HeaderMap, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        assert!(input_schema.get("type") == Some(&json!("object")), "tool {name}: the input schema must be an object schema");
        let tools = Arc::get_mut(&mut self.tools).expect("add tools before serving");
        let handler: Handler = Arc::new(move |headers, args| Box::pin(handler(headers, args)));
        let tool = Tool { description: description.into(), input_schema, read_only, handler };
        assert!(tools.insert(name.into(), tool).is_none(), "tool {name} is defined twice");
        self
    }

    /// The `POST` route for the endpoint, usually mounted at `/mcp`.
    pub fn route<S: Clone + Send + Sync + 'static>(self) -> MethodRouter<S> {
        post(move |headers: HeaderMap, body: Bytes| {
            let tools = self.clone();
            async move { tools.handle(headers, &body).await }
        })
    }

    async fn handle(&self, headers: HeaderMap, body: &[u8]) -> Response {
        let message: Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => return reply(Value::Null, Err((-32700, "parse error".into()))),
        };
        if !message.is_object() {
            // JSON-RPC batches were removed from MCP in 2025-06-18.
            return reply(Value::Null, Err((-32600, "invalid request: send one JSON-RPC message per request".into())));
        }
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let Some(id) = id else {
            // A notification (`notifications/initialized`) or a response: nothing to answer.
            return StatusCode::ACCEPTED.into_response();
        };
        if message.get("jsonrpc") != Some(&json!("2.0")) || method.is_empty() {
            return reply(id, Err((-32600, "invalid request".into())));
        }
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
                let version = PROTOCOL_VERSIONS.iter().find(|v| **v == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": self.name, "version": self.version },
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.list() })),
            "tools/call" => match self.call(headers, params).await {
                Err(CallError::Unauthorized) => {
                    let mut res = reply(id, Err((-32001, "unauthorized".into())));
                    *res.status_mut() = StatusCode::UNAUTHORIZED;
                    res.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
                    return res;
                }
                Err(CallError::Rpc(code, message)) => Err((code, message)),
                Ok(result) => Ok(result),
            },
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        reply(id, result)
    }

    fn list(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|(name, tool)| {
                json!({
                    "name": name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                    "annotations": { "readOnlyHint": tool.read_only },
                })
            })
            .collect()
    }

    async fn call(&self, headers: HeaderMap, params: Value) -> Result<Value, CallError> {
        let name = params.get("name").and_then(Value::as_str).ok_or(CallError::Rpc(-32602, "missing tool name".into()))?;
        let tool = self.tools.get(name).ok_or_else(|| CallError::Rpc(-32602, format!("unknown tool: {name}")))?;
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !args.is_object() {
            return Err(CallError::Rpc(-32602, "arguments must be an object".into()));
        }
        // Tool failures are results with isError, so the agent can read them and retry.
        let failed = |text: String| json!({ "content": [{ "type": "text", "text": text }], "isError": true });
        match (tool.handler)(headers, args).await {
            Ok(value) => {
                let text = value.to_string();
                let structured = if value.is_object() { value } else { json!({ "result": value }) };
                Ok(json!({ "content": [{ "type": "text", "text": text }], "structuredContent": structured, "isError": false }))
            }
            Err(ToolError::Unauthorized) => Err(CallError::Unauthorized),
            Err(ToolError::Invalid(message)) => Ok(failed(format!("invalid arguments: {message}"))),
            Err(ToolError::Forbidden(message)) => Ok(failed(format!("not allowed: {message}"))),
            Err(ToolError::Internal(error)) => {
                tracing::error!(tool = name, %error, "corten: agent tool failed");
                Ok(failed("internal error".into()))
            }
        }
    }
}

enum CallError {
    Unauthorized,
    Rpc(i64, String),
}

fn reply(id: Value, result: Result<Value, (i64, String)>) -> Response {
    let body = match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    };
    ([(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))], body.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app() -> Router {
        let schema = json!({ "type": "object", "properties": { "id": { "type": "string" }, "delta": { "type": "integer" } }, "required": ["id", "delta"] });
        let tools = Tools::new("test", "1.0")
            .tool("inventory.adjust", "Adjust quantity", schema, |headers, args| async move {
                if headers.get("authorization").is_none() {
                    return Err(ToolError::Unauthorized);
                }
                #[derive(serde::Deserialize)]
                struct Adjust {
                    id: String,
                    delta: i64,
                }
                let input: Adjust = serde_json::from_value(args)?;
                if input.id == "locked" {
                    return Err(ToolError::Forbidden("item is locked".into()));
                }
                Ok(json!({ "id": input.id, "quantity": 10 + input.delta }))
            })
            .read_tool("ping", "Answers pong", json!({ "type": "object" }), |_, _| async { Ok(json!("pong")) });
        Router::new().route("/mcp", tools.route())
    }

    async fn rpc(body: Value, auth: bool) -> (StatusCode, Value) {
        let mut req = Request::post("/mcp").header("content-type", "application/json");
        if auth {
            req = req.header("authorization", "Bearer t");
        }
        let res = app().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() })
    }

    #[tokio::test]
    async fn initialize_negotiates_the_protocol_version() {
        let (_, res) = rpc(
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
            false,
        )
        .await;
        assert_eq!(res["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(res["result"]["serverInfo"]["name"], "test");
        let (_, res) = rpc(
            json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }),
            false,
        )
        .await;
        assert_eq!(res["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        let (status, _) = rpc(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), false).await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn lists_and_calls_tools() {
        let (_, res) = rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }), false).await;
        let tools = res["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "inventory.adjust");
        assert_eq!(tools[1]["annotations"]["readOnlyHint"], true);
        let call = |args: Value| json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": "inventory.adjust", "arguments": args } });
        let (_, res) = rpc(call(json!({ "id": "a", "delta": 2 })), true).await;
        assert_eq!(res["id"], 7);
        assert_eq!(res["result"]["structuredContent"], json!({ "id": "a", "quantity": 12 }));
        assert_eq!(res["result"]["isError"], false);
        let (_, res) = rpc(call(json!({ "id": "a" })), true).await;
        assert_eq!(res["result"]["isError"], true);
        assert!(res["result"]["content"][0]["text"].as_str().unwrap().starts_with("invalid arguments: missing field `delta`"));
        let (_, res) = rpc(call(json!({ "id": "locked", "delta": 1 })), true).await;
        assert_eq!(res["result"]["content"][0]["text"], "not allowed: item is locked");
        let (status, res) = rpc(call(json!({ "id": "a", "delta": 1 })), false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(res["error"]["code"], -32001);
    }

    #[tokio::test]
    async fn bad_requests_get_json_rpc_errors() {
        let (_, res) = rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "nope" }), false).await;
        assert_eq!(res["error"]["code"], -32601);
        let (_, res) =
            rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "missing" } }), false).await;
        assert_eq!(res["error"]["code"], -32602);
        let res = app().oneshot(Request::post("/mcp").body(Body::from("{")).unwrap()).await.unwrap();
        let body: Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(body["error"]["code"], -32700);
        let (status, res) = rpc(json!([{ "jsonrpc": "2.0", "id": 1, "method": "ping" }]), false).await;
        assert_eq!((status, res["error"]["code"].clone()), (StatusCode::OK, json!(-32600)), "batches are refused, not accepted");
    }
}
