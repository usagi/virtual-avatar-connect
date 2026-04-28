//! OBS WebSocket v5 連携ノード。
//!
//! Phase σ の first slice。OBS 側で WebSocket Server を有効化している前提で、
//! `ws://127.0.0.1:4455` へ短命接続し、1 request を送って閉じる。

use crate::flowgraph::node::{
	get_optional_int, get_optional_string, get_required_json, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap,
	NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

static REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
struct ObsRequest {
	url: String,
	password: String,
	request_type: String,
	request_data: JsonValue,
	timeout_ms: i64,
}

#[derive(Debug, Clone)]
struct ObsResponse {
	status_code: i64,
	response_data: JsonValue,
}

pub struct ObsRequestNode;

impl NodeDescriptor for ObsRequestNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.obs.request".into(),
			title: "OBS: Request".into(),
			category: "obs".into(),
			description: Some("OBS WebSocket v5 に 1 request を送る汎用ノード。OBS 側で WebSocket Server を有効化しておく。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("url", "URL", SocketType::String).with_default(SocketValue::String("ws://127.0.0.1:4455".into())),
				PortSpec::input("password", "Password", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("request_type", "Request Type", SocketType::String),
				PortSpec::input("request_data", "Request Data", SocketType::Json)
					.with_default(SocketValue::Json(JsonValue::Object(Default::default()))),
				PortSpec::input("timeout_ms", "Timeout ms", SocketType::Int).with_default(SocketValue::Int(3000)),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("ok", "OK", SocketType::Bool),
				PortSpec::output("status_code", "Status Code", SocketType::Int),
				PortSpec::output("response", "Response", SocketType::Json),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ObsRequestNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let req = ObsRequest {
			url: get_optional_string(inputs, "url", "ws://127.0.0.1:4455")?,
			password: get_optional_string(inputs, "password", "")?,
			request_type: get_required_string(inputs, "request_type")?,
			request_data: get_required_json(inputs, "request_data")?.clone(),
			timeout_ms: get_optional_int(inputs, "timeout_ms", 3000)?,
		};
		execute_obs_request_node(ctx, req).await
	}
}

pub struct ObsSetCurrentProgramSceneNode;

impl NodeDescriptor for ObsSetCurrentProgramSceneNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.obs.set_current_program_scene".into(),
			title: "OBS: Set Current Program Scene".into(),
			category: "obs".into(),
			description: Some("OBS WebSocket v5 の `SetCurrentProgramScene` を呼び、現在の番組シーンを切り替える。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("url", "URL", SocketType::String).with_default(SocketValue::String("ws://127.0.0.1:4455".into())),
				PortSpec::input("password", "Password", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("scene_name", "Scene Name", SocketType::String),
				PortSpec::input("timeout_ms", "Timeout ms", SocketType::Int).with_default(SocketValue::Int(3000)),
			],
			outputs: obs_common_outputs(),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ObsSetCurrentProgramSceneNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let scene_name = get_required_string(inputs, "scene_name")?;
		let req = ObsRequest {
			url: get_optional_string(inputs, "url", "ws://127.0.0.1:4455")?,
			password: get_optional_string(inputs, "password", "")?,
			request_type: "SetCurrentProgramScene".into(),
			request_data: json!({ "sceneName": scene_name }),
			timeout_ms: get_optional_int(inputs, "timeout_ms", 3000)?,
		};
		execute_obs_request_node(ctx, req).await
	}
}

fn obs_common_outputs() -> Vec<PortSpec> {
	vec![
		PortSpec::exec_output("exec_out", "On Success"),
		PortSpec::exec_output("on_error", "On Error"),
		PortSpec::output("ok", "OK", SocketType::Bool),
		PortSpec::output("status_code", "Status Code", SocketType::Int),
		PortSpec::output("response", "Response", SocketType::Json),
		PortSpec::output("error", "Error", SocketType::String),
	]
}

async fn execute_obs_request_node(ctx: &mut ExecCtx, req: ObsRequest) -> Result<NodeOutput, NodeExecError> {
	match call_obs_with_timeout(req.clone()).await {
		Ok(resp) => {
			ctx.log(format!("obs.request: {} ok status={}", req.request_type, resp.status_code));
			Ok(NodeOutput::new()
				.set_data("ok", SocketValue::Bool(true))
				.set_data("status_code", SocketValue::Int(resp.status_code))
				.set_data("response", SocketValue::Json(resp.response_data))
				.set_data("error", SocketValue::String(String::new()))
				.fire_exec("exec_out"))
		}
		Err(e) => {
			ctx.log(format!("obs.request: {} error: {e}", req.request_type));
			Ok(NodeOutput::new()
				.set_data("ok", SocketValue::Bool(false))
				.set_data("status_code", SocketValue::Int(0))
				.set_data("response", SocketValue::Json(JsonValue::Null))
				.set_data("error", SocketValue::String(e))
				.fire_exec("on_error"))
		}
	}
}

async fn call_obs_with_timeout(req: ObsRequest) -> Result<ObsResponse, String> {
	let timeout_ms = req.timeout_ms.clamp(100, 60_000) as u64;
	tokio::time::timeout(Duration::from_millis(timeout_ms), call_obs(req))
		.await
		.map_err(|_| format!("timeout after {timeout_ms}ms"))?
}

async fn call_obs(req: ObsRequest) -> Result<ObsResponse, String> {
	if req.request_type.trim().is_empty() {
		return Err("request_type is empty".into());
	}
	let (mut ws, _) = tokio_tungstenite::connect_async(req.url.trim())
		.await
		.map_err(|e| format!("connect failed: {e}"))?;

	let hello = next_json_message(&mut ws).await?;
	let auth = match hello.get("d").and_then(|d| d.get("authentication")) {
		Some(auth) => make_auth(&req.password)(auth)?,
		None => None,
	};
	let mut identify = json!({
		"op": 1,
		"d": {
			"rpcVersion": 1
		}
	});
	if let Some(auth) = auth {
		identify["d"]["authentication"] = JsonValue::String(auth);
	}
	send_json_message(&mut ws, &identify).await?;
	wait_for_op(&mut ws, 2).await?;

	let request_id = format!("vac-{}", REQUEST_SEQ.fetch_add(1, Ordering::Relaxed));
	let request = json!({
		"op": 6,
		"d": {
			"requestType": req.request_type,
			"requestId": request_id,
			"requestData": req.request_data
		}
	});
	send_json_message(&mut ws, &request).await?;
	loop {
		let msg = next_json_message(&mut ws).await?;
		if msg.get("op").and_then(JsonValue::as_i64) != Some(7) {
			continue;
		}
		let d = msg.get("d").ok_or_else(|| "request response missing d".to_string())?;
		if d.get("requestId").and_then(JsonValue::as_str) != Some(request_id.as_str()) {
			continue;
		}
		let status = d
			.get("requestStatus")
			.ok_or_else(|| "request response missing requestStatus".to_string())?;
		let result = status.get("result").and_then(JsonValue::as_bool).unwrap_or(false);
		let code = status.get("code").and_then(JsonValue::as_i64).unwrap_or(0);
		if !result {
			let comment = status.get("comment").and_then(JsonValue::as_str).unwrap_or("request failed");
			return Err(format!("OBS request failed code={code}: {comment}"));
		}
		let response_data = d.get("responseData").cloned().unwrap_or(JsonValue::Null);
		return Ok(ObsResponse {
			status_code: code,
			response_data,
		});
	}
}

fn make_auth(password: &str) -> impl FnOnce(&JsonValue) -> Result<Option<String>, String> + '_ {
	move |auth| {
		let Some(challenge) = auth.get("challenge").and_then(JsonValue::as_str) else {
			return Ok(None);
		};
		let Some(salt) = auth.get("salt").and_then(JsonValue::as_str) else {
			return Ok(None);
		};
		if password.is_empty() {
			return Err("OBS authentication is required but password input is empty".into());
		}
		Ok(Some(obs_auth_response(password, salt, challenge)))
	}
}

fn obs_auth_response(password: &str, salt: &str, challenge: &str) -> String {
	let secret = STANDARD.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
	STANDARD.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}

async fn send_json_message<S>(ws: &mut S, v: &JsonValue) -> Result<(), String>
where
	S: SinkExt<Message> + Unpin,
	<S as futures_util::Sink<Message>>::Error: std::fmt::Display,
{
	ws.send(Message::Text(v.to_string().into()))
		.await
		.map_err(|e| format!("websocket send failed: {e}"))
}

async fn wait_for_op<S>(ws: &mut S, op: i64) -> Result<(), String>
where
	S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
	loop {
		let msg = next_json_message(ws).await?;
		if msg.get("op").and_then(JsonValue::as_i64) == Some(op) {
			return Ok(());
		}
	}
}

async fn next_json_message<S>(ws: &mut S) -> Result<JsonValue, String>
where
	S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
	while let Some(msg) = ws.next().await {
		let msg = msg.map_err(|e| format!("websocket receive failed: {e}"))?;
		match msg {
			Message::Text(text) => {
				return serde_json::from_str(text.as_str()).map_err(|e| format!("websocket JSON decode failed: {e}"));
			}
			Message::Binary(bytes) => {
				return serde_json::from_slice(&bytes).map_err(|e| format!("websocket binary JSON decode failed: {e}"));
			}
			Message::Close(_) => return Err("websocket closed".into()),
			Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
		}
	}
	Err("websocket ended".into())
}

#[cfg(test)]
mod tests {
	use super::*;
	use tokio::net::TcpListener;

	#[test]
	fn obs_auth_response_is_stable() {
		let a = obs_auth_response("password", "salt", "challenge");
		let b = obs_auth_response("password", "salt", "challenge");
		assert_eq!(a, b);
		assert!(!a.is_empty());
	}

	#[tokio::test]
	async fn request_node_no_fire_is_noop() {
		let mut ctx = ExecCtx::default();
		let out = ObsRequestNode
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[tokio::test]
	async fn scene_node_missing_scene_errors_before_network() {
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let err = ObsSetCurrentProgramSceneNode
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &fired)
			.await
			.unwrap_err();
		assert!(matches!(err, NodeExecError::MissingRequiredInput(_)));
	}

	#[tokio::test]
	async fn call_obs_roundtrips_mock_websocket() {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();
		let server = tokio::spawn(async move {
			let (stream, _) = listener.accept().await.unwrap();
			let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
			ws.send(Message::Text(json!({"op": 0, "d": {"rpcVersion": 1}}).to_string().into()))
				.await
				.unwrap();
			let identify = next_json_message(&mut ws).await.unwrap();
			assert_eq!(identify.get("op").and_then(JsonValue::as_i64), Some(1));
			ws.send(Message::Text(json!({"op": 2, "d": {}}).to_string().into())).await.unwrap();
			let request = next_json_message(&mut ws).await.unwrap();
			assert_eq!(request.get("op").and_then(JsonValue::as_i64), Some(6));
			assert_eq!(request["d"]["requestType"], "SetCurrentProgramScene");
			assert_eq!(request["d"]["requestData"]["sceneName"], "Scene-Gaming");
			let request_id = request["d"]["requestId"].as_str().unwrap();
			ws.send(Message::Text(
				json!({
					"op": 7,
					"d": {
						"requestId": request_id,
						"requestStatus": { "result": true, "code": 100 },
						"responseData": { "ok": true }
					}
				})
				.to_string()
				.into(),
			))
			.await
			.unwrap();
		});
		let resp = call_obs_with_timeout(ObsRequest {
			url: format!("ws://{addr}"),
			password: String::new(),
			request_type: "SetCurrentProgramScene".into(),
			request_data: json!({ "sceneName": "Scene-Gaming" }),
			timeout_ms: 3000,
		})
		.await
		.unwrap();
		assert_eq!(resp.status_code, 100);
		assert_eq!(resp.response_data["ok"], true);
		server.await.unwrap();
	}
}
