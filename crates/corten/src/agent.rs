//! Agent tools over the Model Context Protocol (feature `agent`): one `POST` route that speaks
//! JSON-RPC 2.0 as MCP's streamable HTTP transport defines it, answering with `application/json`.
//!
//! A tool is one of the app's actions with a name, a description, a typed input and a typed
//! output. Both derive `schemars::JsonSchema` (the app depends on `schemars = "1"`), so the
//! `inputSchema` and `outputSchema` agents see are generated from the same structs the
//! handler receives and returns, and cannot drift from them. A form's input struct can be the
//! tool's input as is, so the browser and the agent share one definition and one set of rules.
//!
//! Agents authenticate the way any other client does: a bearer token, never ambient cookies.
//! [`Tools::auth`] runs once per call, and its result is handed to the tool:
//!
//! ```ignore
//! let tools = Tools::new("card-market", env!("CARGO_PKG_VERSION"))
//!     .auth(|headers| auth::bearer(headers).ok_or(ToolError::Unauthorized))
//!     .tool("inventory.adjust", "Change the quantity of an inventory item", |user: User, input: Adjust| async move {
//!         Ok(Adjusted { quantity: inventory::adjust(&user, input).await? })
//!     });
//! // #[corten::routes(api = Mcp)] mod mcp { #[route("/mcp")] pub struct Mcp; }
//! let app = App::new(url::router()).api(|routes| mcp::api(routes).route::<mcp::Mcp>(tools.route()));
//! ```
//!
//! A screen's data is already available to agents through [`crate::Req::screen`]; tools cover
//! the writes. Results go back as `structuredContent` and as the same JSON in a text block, for
//! clients that only read text. The output is serialized once and written into the response as
//! is, so a tool returning a large borrowed value (`Arc<T>` with serde's `rc` feature) costs one
//! serialization and no copy.

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use axum::{
    body::Bytes,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use schemars::{generate::SchemaSettings, JsonSchema};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

/// Protocol revisions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];

/// Why a tool call failed.
#[derive(Debug)]
pub enum ToolError {
    /// The arguments are wrong; the message goes back to the agent so it can correct them.
    Invalid(String),
    /// The arguments are fine, but what they act on has moved on (a stale revision, a record
    /// deleted meanwhile) or other work on it is under way
    /// ([`Errors::refuse`](crate::Errors::refuse)); the message goes back to the agent so it
    /// reads the current state (or, as the message says, waits for that work) before trying
    /// again.
    Refused(String),
    /// A service the tool calls failed ([`Refusal::Upstream`](crate::Refusal::Upstream)); the
    /// message goes back to the agent, which may try again later.
    Upstream(String),
    /// No valid credentials: the response is HTTP 401 so the client can authenticate.
    Unauthorized,
    /// Authenticated but not allowed; the message goes back to the agent.
    Forbidden(String),
    /// Anything else. Logged; the agent only sees "internal error".
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

impl ToolError {
    /// An unexpected failure (a database error, say): logged, and the agent sees "internal error".
    pub fn internal(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        ToolError::Internal(error.into())
    }
}

/// Validation errors go back to the agent as one message per field, so it can correct them; a
/// refusal for a state that moved on or work under way ([`Errors::refuse`](crate::Errors::refuse))
/// as that.
impl From<crate::Errors> for ToolError {
    fn from(errors: crate::Errors) -> Self {
        match errors.refusal() {
            crate::Refusal::Invalid => ToolError::Invalid(errors.to_string()),
            crate::Refusal::Conflict | crate::Refusal::Gone => ToolError::Refused(errors.to_string()),
            crate::Refusal::Upstream => ToolError::Upstream(errors.to_string()),
        }
    }
}

impl From<serde_json::Error> for ToolError {
    fn from(error: serde_json::Error) -> Self {
        ToolError::Invalid(error.to_string())
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, ToolError>> + Send>>;
/// A tool: its output as JSON text.
type Handler = Arc<dyn Fn(HeaderMap, Value) -> BoxFuture<String> + Send + Sync>;
type Auth<A> = Arc<dyn Fn(&HeaderMap) -> Result<A, ToolError> + Send + Sync>;

struct Tool {
    description: String,
    input_schema: Value,
    output_schema: Value,
    read_only: bool,
    handler: Handler,
}

/// The tools one MCP endpoint offers. `A` is what [`Tools::auth`] proves about the caller (a
/// user, a scope); it is `()` until `auth` is set.
pub struct Tools<A = ()> {
    name: String,
    version: String,
    auth: Auth<A>,
    tools: Arc<BTreeMap<String, Tool>>,
}

impl<A> Clone for Tools<A> {
    fn clone(&self) -> Self {
        Tools { name: self.name.clone(), version: self.version.clone(), auth: self.auth.clone(), tools: self.tools.clone() }
    }
}

impl Tools<()> {
    /// An endpoint whose tools anyone may call, until [`Tools::auth`] says otherwise.
    pub fn new(name: &str, version: &str) -> Self {
        Tools { name: name.into(), version: version.into(), auth: Arc::new(|_| Ok(())), tools: Arc::default() }
    }

    /// Checks every tool call before it runs: `Err(ToolError::Unauthorized)` answers 401, and
    /// `Ok(caller)` is passed to the tool. Set it before adding tools.
    pub fn auth<A, F>(self, auth: F) -> Tools<A>
    where
        F: Fn(&HeaderMap) -> Result<A, ToolError> + Send + Sync + 'static,
    {
        assert!(self.tools.is_empty(), "set Tools::auth before adding tools");
        Tools { name: self.name, version: self.version, auth: Arc::new(auth), tools: self.tools }
    }
}

/// JSON Schema for a tool's input (as the server reads it) or output (as it writes it). MCP
/// requires an object at the top.
fn schema<T: JsonSchema>(serialize: bool) -> Value {
    let settings = SchemaSettings::draft2020_12();
    let settings = if serialize { settings.for_serialize() } else { settings.for_deserialize() };
    let mut schema = settings.into_generator().into_root_schema_for::<T>().to_value();
    if let Some(map) = schema.as_object_mut() {
        map.remove("$schema");
        map.remove("title");
    }
    schema
}

impl<A: Send + 'static> Tools<A> {
    /// Adds a tool that changes data.
    pub fn tool<I, O, F, Fut>(self, name: &str, description: &str, handler: F) -> Self
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + 'static,
        F: Fn(A, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, ToolError>> + Send + 'static,
    {
        self.add(name, description, false, handler)
    }

    /// Adds a tool that only reads (marked `readOnlyHint`, so clients may call it without asking).
    pub fn read_tool<I, O, F, Fut>(self, name: &str, description: &str, handler: F) -> Self
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + 'static,
        F: Fn(A, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, ToolError>> + Send + 'static,
    {
        self.add(name, description, true, handler)
    }

    fn add<I, O, F, Fut>(mut self, name: &str, description: &str, read_only: bool, handler: F) -> Self
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + 'static,
        F: Fn(A, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, ToolError>> + Send + 'static,
    {
        let input_schema = schema::<I>(false);
        assert!(
            input_schema.get("type") == Some(&json!("object")),
            "tool {name}: the input must be a struct (MCP input schemas are objects)"
        );
        let output_schema = schema::<O>(true);
        assert!(
            output_schema.get("type") == Some(&json!("object")),
            "tool {name}: the output must be a struct (MCP structured content is an object)"
        );
        let auth = self.auth.clone();
        let handler = Arc::new(handler);
        let handler: Handler = Arc::new(move |headers, args| {
            let caller = auth(&headers);
            let handler = handler.clone();
            Box::pin(async move {
                let caller = caller?;
                let input: I = serde_json::from_value(args)?;
                let output = handler(caller, input).await?;
                serde_json::to_string(&output).map_err(ToolError::internal)
            })
        });
        let tools = Arc::get_mut(&mut self.tools).expect("add tools before serving");
        let tool = Tool { description: description.into(), input_schema, output_schema, read_only, handler };
        assert!(tools.insert(name.into(), tool).is_none(), "tool {name} is defined twice");
        self
    }

    /// The `POST` route for the endpoint, usually mounted at `/mcp` under [`crate::App::api`]:
    /// `.api(|routes| mcp::api(routes).route::<mcp::Mcp>(tools.route()))`.
    pub fn route<S: Clone + Send + Sync + 'static>(self) -> crate::Methods<S>
    where
        A: 'static,
    {
        // Corten's own handler: it answers JSON-RPC errors in the protocol's shape itself.
        crate::Methods::writes(post(move |headers: HeaderMap, body: Bytes| {
            let tools = self.clone();
            async move { tools.handle(headers, &body).await }
        }))
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
            "tools/call" => match self.call(headers, params).await {
                Err(CallError::Unauthorized) => {
                    let mut res = reply(id, Err((-32001, "unauthorized".into())));
                    *res.status_mut() = StatusCode::UNAUTHORIZED;
                    res.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
                    return res;
                }
                Err(CallError::Rpc(code, message)) => Err((code, message)),
                Ok(result) => return reply_raw(&id, &result),
            },
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
                    "outputSchema": tool.output_schema,
                    "annotations": { "readOnlyHint": tool.read_only },
                })
            })
            .collect()
    }

    /// The call's `result`, as JSON text.
    async fn call(&self, headers: HeaderMap, params: Value) -> Result<String, CallError> {
        let name = params.get("name").and_then(Value::as_str).ok_or(CallError::Rpc(-32602, "missing tool name".into()))?;
        let tool = self.tools.get(name).ok_or_else(|| CallError::Rpc(-32602, format!("unknown tool: {name}")))?;
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !args.is_object() {
            return Err(CallError::Rpc(-32602, "arguments must be an object".into()));
        }
        // Tool failures are results with isError, so the agent can read them and retry.
        let failed = |text: String| json!({ "content": [{ "type": "text", "text": text }], "isError": true }).to_string();
        match (tool.handler)(headers, args).await {
            Ok(output) => {
                // The output as written, and once more as a JSON string for the text block, into
                // one string sized for both: a large result is not copied as it grows.
                let text = serde_json::to_string(&output).map_err(|e| CallError::Rpc(-32603, e.to_string()))?;
                Ok([
                    r#"{"content":[{"type":"text","text":"#,
                    &text,
                    r#"}],"structuredContent":"#,
                    &output,
                    r#","isError":false}"#,
                ]
                .concat())
            }
            Err(ToolError::Unauthorized) => Err(CallError::Unauthorized),
            Err(ToolError::Invalid(message)) => Ok(failed(format!("invalid arguments: {message}"))),
            Err(ToolError::Refused(message)) => Ok(failed(format!("refused, read the current state first: {message}"))),
            Err(ToolError::Upstream(message)) => Ok(failed(format!("a service it calls failed, try again later: {message}"))),
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
    json_response(body.to_string())
}

/// A successful reply whose `result` is JSON text already, written into the body as is.
fn reply_raw(id: &Value, result: &str) -> Response {
    json_response([r#"{"id":"#, &id.to_string(), r#","jsonrpc":"2.0","result":"#, result, "}"].concat())
}

fn json_response(body: String) -> Response {
    ([(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))], body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Change an item's quantity.
    #[derive(serde::Deserialize, JsonSchema)]
    struct Adjust {
        id: String,
        /// Added to the quantity; negative to remove.
        delta: i64,
        #[serde(default)]
        note: Option<String>,
    }

    #[derive(Serialize, JsonSchema)]
    struct Adjusted {
        id: String,
        quantity: i64,
    }

    #[derive(serde::Deserialize, JsonSchema)]
    struct Nothing {}

    #[test]
    fn a_refusal_reaches_an_agent_as_what_it_can_do_about_it() {
        let refused = |why, text: &str| {
            let mut errors = crate::Errors::new();
            errors.refuse(why, text);
            ToolError::from(errors)
        };
        assert!(matches!(refused(crate::Refusal::Conflict, "moved"), ToolError::Refused(m) if m == "moved"));
        assert!(matches!(refused(crate::Refusal::Gone, "gone"), ToolError::Refused(m) if m == "gone"));
        assert!(matches!(refused(crate::Refusal::Upstream, "no answer"), ToolError::Upstream(m) if m == "no answer"));
    }

    #[derive(Serialize, JsonSchema)]
    struct Pong {
        pong: bool,
    }

    /// The caller, as the auth check proves it.
    #[derive(Clone)]
    struct Agent(String);

    fn app() -> Router {
        let tools = Tools::new("test", "1.0")
            .auth(|headers| {
                let token = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
                token.map(|t| Agent(t.to_string())).ok_or(ToolError::Unauthorized)
            })
            .tool("inventory.adjust", "Adjust quantity", |agent: Agent, input: Adjust| async move {
                if input.id == "locked" {
                    return Err(ToolError::Forbidden(format!("{} may not change a locked item", agent.0)));
                }
                let _ = input.note;
                Ok(Adjusted { id: input.id, quantity: 10 + input.delta })
            })
            .read_tool("ping", "Answers pong", |_: Agent, _: Nothing| async { Ok(Pong { pong: true }) });
        Router::new().route("/mcp", tools.route().0)
    }

    async fn rpc(body: Value, auth: bool) -> (StatusCode, Value) {
        let mut req = Request::post("/mcp").header("content-type", "application/json");
        if auth {
            req = req.header("authorization", "Bearer ada");
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
    async fn schemas_come_from_the_input_and_output_types() {
        let (_, res) = rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }), false).await;
        let tools = res["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "inventory.adjust");
        assert_eq!(
            tools[0]["inputSchema"],
            json!({
                "description": "Change an item's quantity.",
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "delta": { "description": "Added to the quantity; negative to remove.", "type": "integer", "format": "int64" },
                    "note": { "type": ["string", "null"], "default": null }
                },
                "required": ["id", "delta"]
            })
        );
        assert_eq!(tools[0]["outputSchema"]["required"], json!(["id", "quantity"]));
        assert_eq!(tools[1]["annotations"]["readOnlyHint"], true);
    }

    #[tokio::test]
    async fn calls_are_authorized_then_typed() {
        let call = |args: Value| json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": "inventory.adjust", "arguments": args } });
        let (_, res) = rpc(call(json!({ "id": "a", "delta": 2 })), true).await;
        assert_eq!(res["id"], 7);
        assert_eq!(res["result"]["structuredContent"], json!({ "id": "a", "quantity": 12 }));
        assert_eq!(res["result"]["isError"], false);
        let text: Value = serde_json::from_str(res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, res["result"]["structuredContent"], "the text block is the same JSON");
        let (_, res) = rpc(call(json!({ "id": "a" })), true).await;
        assert_eq!(res["result"]["isError"], true);
        assert!(res["result"]["content"][0]["text"].as_str().unwrap().starts_with("invalid arguments: missing field `delta`"));
        let (_, res) = rpc(call(json!({ "id": "a", "delta": "two" })), true).await;
        assert!(res["result"]["content"][0]["text"].as_str().unwrap().starts_with("invalid arguments: invalid type"));
        let (_, res) = rpc(call(json!({ "id": "locked", "delta": 1 })), true).await;
        assert_eq!(res["result"]["content"][0]["text"], "not allowed: ada may not change a locked item");
        let (status, res) = rpc(call(json!({ "id": "a" })), false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "auth runs before the input is read");
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

    #[test]
    #[should_panic(expected = "the input must be a struct")]
    fn a_non_object_input_is_refused_at_startup() {
        let _ = Tools::new("t", "1").tool("bad", "", |_: (), _: String| async { Ok(Pong { pong: true }) });
    }
}
