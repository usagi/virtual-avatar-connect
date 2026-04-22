//! `flowgraph.ingress.web_input` 用の actix-web 組み込みブリッジ（δ-9 Part B）。
//!
//! ## 機能
//!
//! - ingress ノードの property `path` / `method` / `body_format` / `fixed_channel` を読み、
//!   actix-web の `ServiceConfig` にハンドラを登録する。
//! - リクエスト受信時、本文を decode して `TriggerEvent::new(node_id)` に
//!   `__content__` / `__source_actor__` / `__source_kind__` の overrides を載せ、
//!   `TriggerHandle::send` で Flowgraph ワーカーに投げ込む。
//! - `path` が空の場合は `/input/flowgraph/<fq_node_id>` を自動採番。
//! - 同一 `(path, method)` の重複登録はログ警告のうえ後勝ち。

use crate::flowgraph::loader::LoadedNodeMeta;
use crate::flowgraph::node::{TriggerEvent, TriggerHandle};
use crate::flowgraph::socket::SocketValue;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde::Deserialize;
use std::sync::Arc;

/// ingress ノード 1 件分の解決済みメタ。
#[derive(Debug, Clone)]
pub struct FlowgraphWebInputEndpoint {
	pub node_id: String,
	pub path: String,
	pub method: Method,
	pub body_format: BodyFormat,
	pub fixed_channel: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Method {
	Get,
	Post,
	Put,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyFormat {
	Plain,
	Json,
	Form,
}

impl FlowgraphWebInputEndpoint {
	pub fn from_meta(fq: &str, meta: &LoadedNodeMeta) -> Option<Self> {
		let props = &meta.properties;
		let path_raw = props
			.get("path")
			.and_then(|v| v.as_str().ok())
			.map(str::trim)
			.map(str::to_string)
			.unwrap_or_default();
		let path = if path_raw.is_empty() {
			format!("/input/flowgraph/{}", sanitize_path_segment(fq))
		} else {
			normalize_path(&path_raw)
		};
		let method = props
			.get("method")
			.and_then(|v| v.as_str().ok())
			.map(|s| s.trim().to_ascii_uppercase())
			.and_then(|s| match s.as_str() {
				"GET" => Some(Method::Get),
				"POST" => Some(Method::Post),
				"PUT" => Some(Method::Put),
				other => {
					log::warn!("《Flowgraph/WebInput》 node={fq} 不明な method='{other}'、POST にフォールバック");
					None
				}
			})
			.unwrap_or(Method::Post);
		let body_format = props
			.get("body_format")
			.and_then(|v| v.as_str().ok())
			.map(|s| s.trim().to_ascii_lowercase())
			.and_then(|s| match s.as_str() {
				"plain" | "" => Some(BodyFormat::Plain),
				"json" => Some(BodyFormat::Json),
				"form" => Some(BodyFormat::Form),
				other => {
					log::warn!("《Flowgraph/WebInput》 node={fq} 不明な body_format='{other}'、plain にフォールバック");
					None
				}
			})
			.unwrap_or(BodyFormat::Plain);
		let fixed_channel = props
			.get("fixed_channel")
			.and_then(|v| v.as_str().ok())
			.map(str::trim)
			.map(str::to_string)
			.unwrap_or_default();
		Some(Self {
			node_id: fq.to_string(),
			path,
			method,
			body_format,
			fixed_channel,
		})
	}
}

fn normalize_path(p: &str) -> String {
	let p = p.trim();
	let p = if p.starts_with('/') {
		p.to_string()
	} else {
		format!("/{}", p.trim_start_matches('/'))
	};
	let p = p.trim_end_matches('/');
	if p.is_empty() {
		"/".into()
	} else {
		p.to_string()
	}
}

fn sanitize_path_segment(fq: &str) -> String {
	fq.chars()
		.map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
		.collect()
}

// ---------------------------------------------------------------------
// actix-web 統合
// ---------------------------------------------------------------------

/// Flowgraph web_input エンドポイントを actix-web `ServiceConfig` に一括登録する。
///
/// Trigger は `Arc<Option<TriggerHandle>>` 相当を `Data` で渡す想定。
/// Flowgraph ワーカー未起動時は全ハンドラが 503 を返す。
pub fn register_routes(
	cfg: &mut web::ServiceConfig,
	endpoints: &[FlowgraphWebInputEndpoint],
) {
	use std::collections::HashSet;
	let mut seen: HashSet<(String, Method)> = HashSet::new();
	for ep in endpoints {
		if !seen.insert((ep.path.clone(), ep.method)) {
			log::warn!(
				"《Flowgraph/WebInput》 path+method 重複: {} {:?} (node={})",
				ep.path,
				ep.method,
				ep.node_id
			);
			continue;
		}
		let ep_data = web::Data::new(ep.clone());
		let resource = web::resource(&ep.path);
		let resource = match ep.method {
			Method::Get => resource.route(web::get().to(handle_get)),
			Method::Post => resource.route(web::post().to(handle_post)),
			Method::Put => resource.route(web::put().to(handle_post)), // PUT も POST と同じ decode
		};
		cfg.service(resource.app_data(ep_data));
	}
}

#[derive(Deserialize)]
struct GetQuery {
	#[serde(default)]
	content: Option<String>,
	#[serde(default)]
	source_actor: Option<String>,
}

async fn handle_get(
	_req: HttpRequest,
	query: web::Query<GetQuery>,
	ep: web::Data<FlowgraphWebInputEndpoint>,
	trigger: web::Data<Arc<Option<TriggerHandle>>>,
) -> impl Responder {
	let content = query.content.clone().unwrap_or_default();
	let actor = query.source_actor.clone().unwrap_or_default();
	forward(&ep, &trigger, content, actor).await
}

#[derive(Deserialize)]
struct JsonBody {
	content: String,
	#[serde(default)]
	source_actor: Option<String>,
}

#[derive(Deserialize)]
struct FormBody {
	content: String,
	#[serde(default)]
	source_actor: Option<String>,
}

async fn handle_post(
	_req: HttpRequest,
	body: web::Bytes,
	ep: web::Data<FlowgraphWebInputEndpoint>,
	trigger: web::Data<Arc<Option<TriggerHandle>>>,
) -> impl Responder {
	let (content, actor) = match ep.body_format {
		BodyFormat::Plain => (String::from_utf8_lossy(&body).into_owned(), String::new()),
		BodyFormat::Json => match serde_json::from_slice::<JsonBody>(&body) {
			Ok(b) => (b.content, b.source_actor.unwrap_or_default()),
			Err(e) => {
				return HttpResponse::BadRequest().body(format!("invalid json: {e}"));
			}
		},
		BodyFormat::Form => match serde_urlencoded::from_bytes::<FormBody>(&body) {
			Ok(b) => (b.content, b.source_actor.unwrap_or_default()),
			Err(e) => {
				return HttpResponse::BadRequest().body(format!("invalid form: {e}"));
			}
		},
	};
	forward(&ep, &trigger, content, actor).await
}

async fn forward(
	ep: &FlowgraphWebInputEndpoint,
	trigger: &Arc<Option<TriggerHandle>>,
	content: String,
	actor: String,
) -> HttpResponse {
	let Some(handle) = trigger.as_ref().as_ref() else {
		return HttpResponse::ServiceUnavailable().body("flowgraph runtime not active");
	};
	let source_kind = if ep.fixed_channel.is_empty() {
		"web_input".to_string()
	} else {
		ep.fixed_channel.clone()
	};
	let event = TriggerEvent::new(&ep.node_id)
		.with_exec("__trigger__")
		.with_override("__content__", SocketValue::String(content))
		.with_override("__source_actor__", SocketValue::String(actor))
		.with_override("__source_kind__", SocketValue::String(source_kind));
	match handle.send(event) {
		Ok(()) => HttpResponse::Ok().body("ok"),
		Err(e) => {
			log::warn!(
				"《Flowgraph/WebInput》 trigger 送信失敗 node={} err={e}",
				ep.node_id
			);
			HttpResponse::ServiceUnavailable().body(format!("trigger send failed: {e}"))
		}
	}
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::engine::create_trigger_bus;
	use crate::flowgraph::node::InputMap;
	use std::path::PathBuf;

	fn meta_with(props: Vec<(&str, SocketValue)>) -> LoadedNodeMeta {
		let mut m = InputMap::new();
		for (k, v) in props {
			m.insert(k.to_string(), v);
		}
		LoadedNodeMeta {
			feature: "flowgraph.ingress.web_input".into(),
			file: PathBuf::from("t.flowgraph.toml"),
			position: None,
			properties: m,
		}
	}

	#[test]
	fn endpoint_defaults_are_applied() {
		let meta = meta_with(vec![]);
		let ep = FlowgraphWebInputEndpoint::from_meta("ingress/web::in", &meta).unwrap();
		assert_eq!(ep.path, "/input/flowgraph/ingress_web__in");
		assert_eq!(ep.method, Method::Post);
		assert_eq!(ep.body_format, BodyFormat::Plain);
		assert_eq!(ep.fixed_channel, "");
	}

	#[test]
	fn endpoint_from_properties() {
		let meta = meta_with(vec![
			("path", SocketValue::String("/input/chat".into())),
			("method", SocketValue::String("post".into())),
			("body_format", SocketValue::String("json".into())),
			("fixed_channel", SocketValue::String("my_channel".into())),
		]);
		let ep = FlowgraphWebInputEndpoint::from_meta("in", &meta).unwrap();
		assert_eq!(ep.path, "/input/chat");
		assert_eq!(ep.method, Method::Post);
		assert_eq!(ep.body_format, BodyFormat::Json);
		assert_eq!(ep.fixed_channel, "my_channel");
	}

	#[test]
	fn unknown_method_falls_back_to_post() {
		let meta = meta_with(vec![("method", SocketValue::String("HEAD".into()))]);
		let ep = FlowgraphWebInputEndpoint::from_meta("in", &meta).unwrap();
		assert_eq!(ep.method, Method::Post);
	}

	#[tokio::test]
	async fn forward_sends_trigger_event_with_overrides() {
		let (handle, mut rx) = create_trigger_bus();
		let arc_handle: Arc<Option<TriggerHandle>> = Arc::new(Some(handle));
		let ep = FlowgraphWebInputEndpoint {
			node_id: "in".into(),
			path: "/x".into(),
			method: Method::Post,
			body_format: BodyFormat::Plain,
			fixed_channel: "chan1".into(),
		};
		let _resp = super::forward(&ep, &arc_handle, "hello".into(), "alice".into()).await;
		let ev = rx.recv().await.expect("event received");
		assert_eq!(ev.node_id, "in");
		assert!(ev.fired_exec.contains(&"__trigger__".to_string()));
		assert_eq!(
			ev.data_overrides.get("__content__").and_then(|v| v.as_str().ok()),
			Some("hello")
		);
		assert_eq!(
			ev.data_overrides.get("__source_actor__").and_then(|v| v.as_str().ok()),
			Some("alice")
		);
		assert_eq!(
			ev.data_overrides.get("__source_kind__").and_then(|v| v.as_str().ok()),
			Some("chan1")
		);
	}
}
