//! Phase σ: 汎用 HTTP request ノード。
//!
//! webhook、ローカル補助サービス、外部 REST API を Flowgraph から叩くための first slice。

use crate::flowgraph::node::{
	get_optional_int, get_optional_string, get_required_json, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap,
	NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, RecordedEffect,
};
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value as JsonValue;
use std::time::Duration;

pub struct HttpRequestNode;

#[derive(Debug, Clone)]
struct HttpRequestSpec {
	method: String,
	url: String,
	headers: JsonValue,
	body: JsonValue,
	timeout_ms: i64,
	retry_count: i64,
	retry_delay_ms: i64,
}

#[derive(Debug, Clone)]
struct HttpResponseData {
	status: i64,
	body_text: String,
	body_json: JsonValue,
}

impl NodeDescriptor for HttpRequestNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.http.request".into(),
			title: "HTTP Request".into(),
			category: "http".into(),
			description: Some("汎用 HTTP request。method / headers / JSON body / timeout / retry_count を指定できる。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("method", "Method", SocketType::String).with_default(SocketValue::String("GET".into())),
				PortSpec::input("url", "URL", SocketType::String),
				PortSpec::input("headers", "Headers", SocketType::Json)
					.with_default(SocketValue::Json(JsonValue::Object(Default::default()))),
				PortSpec::input("body", "Body JSON", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
				PortSpec::input("timeout_ms", "Timeout ms", SocketType::Int).with_default(SocketValue::Int(5000)),
				PortSpec::input("retry_count", "Retry count", SocketType::Int).with_default(SocketValue::Int(0)),
				PortSpec::input("retry_delay_ms", "Retry delay ms", SocketType::Int).with_default(SocketValue::Int(250)),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("ok", "OK", SocketType::Bool),
				PortSpec::output("status", "Status", SocketType::Int),
				PortSpec::output("body", "Body", SocketType::String),
				PortSpec::output("json", "JSON", SocketType::Json),
				PortSpec::output("error", "Error", SocketType::String),
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Json))),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for HttpRequestNode {
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
		let spec = HttpRequestSpec {
			method: get_optional_string(inputs, "method", "GET")?,
			url: get_required_string(inputs, "url")?,
			headers: get_required_json(inputs, "headers")?.clone(),
			body: get_required_json(inputs, "body")?.clone(),
			timeout_ms: get_optional_int(inputs, "timeout_ms", 5000)?,
			retry_count: get_optional_int(inputs, "retry_count", 0)?,
			retry_delay_ms: get_optional_int(inputs, "retry_delay_ms", 250)?,
		};
		let response = if let Some(mock) = ctx
			.effect_mocks
			.as_ref()
			.and_then(|mocks| mocks.http_response(&ctx.node_id))
			.cloned()
		{
			ctx.log(format!("http.request mock: {} {} -> node {}", spec.method, spec.url, ctx.node_id));
			match mock.error {
				Some(error) => {
					ctx.record_effect(RecordedEffect::http_request(
						ctx.node_id.clone(),
						spec.method.clone(),
						spec.url.clone(),
						spec.body.clone(),
						None,
						None,
						Some(error.clone()),
					));
					Err(error)
				}
				None => {
					ctx.record_effect(RecordedEffect::http_request(
						ctx.node_id.clone(),
						spec.method.clone(),
						spec.url.clone(),
						spec.body.clone(),
						Some(mock.status),
						Some(mock.body_text.clone()),
						None,
					));
					Ok(HttpResponseData {
						status: mock.status,
						body_text: mock.body_text,
						body_json: mock.body_json,
					})
				}
			}
		} else {
			execute_http_request(spec.clone()).await
		};
		match response {
			Ok(resp) => {
				ctx.log(format!("http.request: {} {} -> {}", spec.method, spec.url, resp.status));
				let ok = (200..=299).contains(&resp.status);
				let result = response_result(&resp, ok);
				let out = NodeOutput::new()
					.set_data("ok", SocketValue::Bool(ok))
					.set_data("status", SocketValue::Int(resp.status))
					.set_data("body", SocketValue::String(resp.body_text.clone()))
					.set_data("json", SocketValue::Json(resp.body_json.clone()))
					.set_data("error", SocketValue::String(String::new()))
					.set_data("result", SocketValue::Result(result));
				Ok(if ok {
					out.fire_exec("on_success")
				} else {
					out.fire_exec("on_error")
				})
			}
			Err(e) => {
				ctx.log(format!("http.request: {} {} error: {e}", spec.method, spec.url));
				Ok(error_output(e))
			}
		}
	}
}

fn error_output(e: impl Into<String>) -> NodeOutput {
	let error = e.into();
	NodeOutput::new()
		.set_data("ok", SocketValue::Bool(false))
		.set_data("status", SocketValue::Int(0))
		.set_data("body", SocketValue::String(String::new()))
		.set_data("json", SocketValue::Json(JsonValue::Null))
		.set_data("error", SocketValue::String(error.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(error).with_code("http.request")))
		.fire_exec("on_error")
}

fn response_result(resp: &HttpResponseData, ok: bool) -> FlowResult {
	let value = SocketValue::Json(serde_json::json!({
		"status": resp.status,
		"body": resp.body_text,
		"json": resp.body_json,
	}));
	if ok {
		FlowResult::ok(value)
	} else {
		FlowResult {
			ok: false,
			value: Some(Box::new(value)),
			error: Some(format!("HTTP {}", resp.status)),
			code: Some("http.status".into()),
		}
	}
}

async fn execute_http_request(spec: HttpRequestSpec) -> Result<HttpResponseData, String> {
	let method = parse_method(&spec.method)?;
	let url = spec.url.trim();
	if url.is_empty() {
		return Err("url is empty".into());
	}
	let headers = parse_headers(&spec.headers)?;
	let timeout_ms = spec.timeout_ms.clamp(100, 120_000) as u64;
	let retry_count = spec.retry_count.clamp(0, 5);
	let retry_delay_ms = spec.retry_delay_ms.clamp(0, 30_000) as u64;
	let client = reqwest::Client::builder()
		.timeout(Duration::from_millis(timeout_ms))
		.build()
		.map_err(|e| format!("reqwest client: {e}"))?;

	let mut last_err: Option<String> = None;
	for attempt in 0..=retry_count {
		match send_once(&client, method.clone(), url, headers.clone(), &spec.body).await {
			Ok(resp) => {
				if resp.status >= 500 && attempt < retry_count {
					last_err = Some(format!("HTTP {}", resp.status));
				} else {
					return Ok(resp);
				}
			}
			Err(e) => {
				if attempt >= retry_count {
					return Err(e);
				}
				last_err = Some(e);
			}
		}
		if retry_delay_ms > 0 {
			tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
		}
	}
	Err(last_err.unwrap_or_else(|| "request failed".into()))
}

async fn send_once(
	client: &reqwest::Client,
	method: reqwest::Method,
	url: &str,
	headers: HeaderMap,
	body: &JsonValue,
) -> Result<HttpResponseData, String> {
	let mut req = client.request(method.clone(), url).headers(headers);
	if !body.is_null() && method != reqwest::Method::GET && method != reqwest::Method::HEAD {
		req = req.json(body);
	}
	let res = req.send().await.map_err(|e| format!("send failed: {e}"))?;
	let status = i64::from(res.status().as_u16());
	let body_text = res.text().await.map_err(|e| format!("read body failed: {e}"))?;
	let body_json = serde_json::from_str(&body_text).unwrap_or(JsonValue::Null);
	Ok(HttpResponseData {
		status,
		body_text,
		body_json,
	})
}

fn parse_method(method: &str) -> Result<reqwest::Method, String> {
	match method.trim().to_ascii_uppercase().as_str() {
		"GET" => Ok(reqwest::Method::GET),
		"POST" => Ok(reqwest::Method::POST),
		"PUT" => Ok(reqwest::Method::PUT),
		"PATCH" => Ok(reqwest::Method::PATCH),
		"DELETE" => Ok(reqwest::Method::DELETE),
		"HEAD" => Ok(reqwest::Method::HEAD),
		other => Err(format!("unsupported method: {other}")),
	}
}

fn parse_headers(v: &JsonValue) -> Result<HeaderMap, String> {
	let mut headers = HeaderMap::new();
	let Some(obj) = v.as_object() else {
		return Err("headers must be a JSON object".into());
	};
	for (k, v) in obj {
		let name = HeaderName::from_bytes(k.as_bytes()).map_err(|e| format!("invalid header name {k:?}: {e}"))?;
		let value_s = match v {
			JsonValue::String(s) => s.clone(),
			JsonValue::Number(_) | JsonValue::Bool(_) => v.to_string(),
			JsonValue::Null => String::new(),
			_ => return Err(format!("header value for {k:?} must be string/number/bool/null")),
		};
		let value = HeaderValue::from_str(&value_s).map_err(|e| format!("invalid header value for {k:?}: {e}"))?;
		headers.insert(name, value);
	}
	Ok(headers)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	use std::collections::HashMap;
	use std::sync::Arc;
	use tokio::io::{AsyncReadExt, AsyncWriteExt};
	use tokio::net::TcpListener;

	#[test]
	fn parse_method_rejects_unknown() {
		assert!(parse_method("TRACE").is_err());
		assert_eq!(parse_method("post").unwrap(), reqwest::Method::POST);
	}

	#[test]
	fn parse_headers_accepts_scalar_values() {
		let headers = parse_headers(&json!({
			"x-a": "alpha",
			"x-b": 2,
			"x-c": true
		}))
		.unwrap();
		assert_eq!(headers.get("x-a").unwrap(), "alpha");
		assert_eq!(headers.get("x-b").unwrap(), "2");
		assert_eq!(headers.get("x-c").unwrap(), "true");
	}

	#[tokio::test]
	async fn request_node_no_fire_is_noop() {
		let mut ctx = ExecCtx::default();
		let out = HttpRequestNode
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[tokio::test]
	async fn request_node_emits_result_for_mock_success() {
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [
			("url".into(), SocketValue::String("https://example.test/hook".into())),
			("headers".into(), SocketValue::Json(json!({}))),
			("body".into(), SocketValue::Json(json!({ "hello": "vac" }))),
		]
		.into_iter()
		.collect();
		let mut http = HashMap::new();
		http.insert(
			"main::http".into(),
			crate::flowgraph::node::HttpMockResponse {
				status: 200,
				body_text: "{\"accepted\":true}".into(),
				body_json: json!({ "accepted": true }),
				error: None,
			},
		);
		let mut ctx = ExecCtx {
			node_id: "main::http".into(),
			effect_mocks: Some(Arc::new(crate::flowgraph::node::EffectMocks {
				http,
				..Default::default()
			})),
			..Default::default()
		};

		let out = HttpRequestNode.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_success"));
		let result = out.data.get("result").unwrap().as_result().unwrap();
		assert!(result.ok);
		let value = result.value.as_ref().unwrap().as_json().unwrap();
		assert_eq!(value["status"], 200);
		assert_eq!(value["json"]["accepted"], true);
	}

	#[tokio::test]
	async fn request_node_emits_result_for_mock_error() {
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [
			("url".into(), SocketValue::String("https://example.test/hook".into())),
			("headers".into(), SocketValue::Json(json!({}))),
			("body".into(), SocketValue::Json(JsonValue::Null)),
		]
		.into_iter()
		.collect();
		let mut http = HashMap::new();
		http.insert(
			"main::http".into(),
			crate::flowgraph::node::HttpMockResponse {
				status: 0,
				body_text: String::new(),
				body_json: JsonValue::Null,
				error: Some("network down".into()),
			},
		);
		let mut ctx = ExecCtx {
			node_id: "main::http".into(),
			effect_mocks: Some(Arc::new(crate::flowgraph::node::EffectMocks {
				http,
				..Default::default()
			})),
			..Default::default()
		};

		let out = HttpRequestNode.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let result = out.data.get("result").unwrap().as_result().unwrap();
		assert!(!result.ok);
		assert_eq!(result.error.as_deref(), Some("network down"));
		assert_eq!(result.code.as_deref(), Some("http.request"));
	}

	#[tokio::test]
	async fn http_request_roundtrips_json_response() {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();
		let server = tokio::spawn(async move {
			let (mut socket, _) = listener.accept().await.unwrap();
			let mut buf = vec![0; 4096];
			let n = socket.read(&mut buf).await.unwrap();
			let request = String::from_utf8_lossy(&buf[..n]);
			assert!(request.starts_with("POST /hook HTTP/1.1"));
			assert!(request.contains("x-test: ok"));
			let body = r#"{"accepted":true}"#;
			let response = format!(
				"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
				body.len(),
				body
			);
			socket.write_all(response.as_bytes()).await.unwrap();
		});
		let resp = execute_http_request(HttpRequestSpec {
			method: "POST".into(),
			url: format!("http://{addr}/hook"),
			headers: json!({ "x-test": "ok" }),
			body: json!({ "hello": "vac" }),
			timeout_ms: 3000,
			retry_count: 0,
			retry_delay_ms: 0,
		})
		.await
		.unwrap();
		assert_eq!(resp.status, 200);
		assert_eq!(resp.body_json["accepted"], true);
		server.await.unwrap();
	}
}
