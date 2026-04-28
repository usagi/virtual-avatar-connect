//! 《WebInput》: HTTP によるチャンネルパイプラインへの入力起点。
//! - 待受アドレスは全体設定の `web_ui_address`（デフォルト `127.0.0.1:57000`）に従う。
//! - `[[processors]] feature = "webinput"` でパス・メソッド・POST 時の `body_format` を宣言。
//! - 宣言が無いときは `POST /input` + JSON のみ（レガシー互換）。

use crate::conf::Conf;
use crate::resource::CONTENT_TYPE_APPLICATION_JSON;
use crate::shutdown::ShutdownReason;
use crate::{ChannelDatum, Result as VacResult, SharedState};
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputPayload {
	/// `fixed_channel` 未設定のエンドポイントでは必須（GET はクエリ、POST はボディ）。
	#[serde(default)]
	pub channel: Option<String>,
	pub content: String,
	#[serde(default)]
	pub is_final: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebInputMethod {
	Get,
	Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebInputBodyFormat {
	Json,
	Toml,
	MsgPack,
}

#[derive(Debug, Clone)]
pub struct WebInputEndpoint {
	pub path: String,
	/// 指定時は `channel` を無視し、常にこの VAC チャンネルへ流す。
	pub fixed_channel: Option<String>,
	pub method: WebInputMethod,
	/// POST のときのみ使用。GET では無視。
	pub body_format: WebInputBodyFormat,
}

#[derive(Debug, Clone)]
pub struct WebInputRegistry {
	endpoints: Vec<WebInputEndpoint>,
}

enum WebInputCommandOutcome {
	Continue,
	Handled(HttpResponse),
}

impl WebInputRegistry {
	pub fn from_conf(conf: &Conf) -> anyhow::Result<Self> {
		let mut endpoints = Vec::new();
		for pc in &conf.processors {
			if !pc.is_enabled {
				continue;
			}
			if !matches!(pc.feature.as_deref().map(|s| s.eq_ignore_ascii_case("webinput")), Some(true)) {
				continue;
			}
			let path = normalize_web_path(pc.web_input_path.as_deref().unwrap_or("/input"));
			let method = parse_web_input_method(pc.web_input_method.as_deref());
			let body_format = parse_web_input_body_format(pc.web_input_body_format.as_deref());
			if method == WebInputMethod::Get && body_format != WebInputBodyFormat::Json {
				log::warn!(
					"《WebInput》: GET では web_input_body_format は無視されます（常にクエリ） id={:?}",
					pc.id
				);
			}
			endpoints.push(WebInputEndpoint {
				path,
				fixed_channel: pc.channel_to.clone().filter(|s| !s.trim().is_empty()),
				method,
				body_format,
			});
		}

		if endpoints.is_empty() {
			return Ok(Self {
				endpoints: vec![WebInputEndpoint {
					path: normalize_web_path("/input"),
					fixed_channel: None,
					method: WebInputMethod::Post,
					body_format: WebInputBodyFormat::Json,
				}],
			});
		}

		let mut seen = HashSet::new();
		for ep in &endpoints {
			if !seen.insert((ep.path.clone(), ep.method)) {
				anyhow::bail!(
					"《WebInput》: 同じ web_input_path と method の組が重複しています: {} {:?}",
					ep.path,
					ep.method
				);
			}
		}

		Ok(Self { endpoints })
	}

	pub fn resolve(&self, path: &str, method: WebInputMethod) -> Option<&WebInputEndpoint> {
		let n = normalize_web_path(path);
		self.endpoints.iter().find(|e| e.path == n && e.method == method)
	}
}

fn parse_web_input_method(s: Option<&str>) -> WebInputMethod {
	match s.map(str::trim).filter(|x| !x.is_empty()).map(|x| x.to_lowercase()).as_deref() {
		Some("get") => WebInputMethod::Get,
		_ => WebInputMethod::Post,
	}
}

fn parse_web_input_body_format(s: Option<&str>) -> WebInputBodyFormat {
	match s.map(str::trim).filter(|x| !x.is_empty()).map(|x| x.to_lowercase()).as_deref() {
		Some("toml") => WebInputBodyFormat::Toml,
		Some("msgpack") | Some("messagepack") => WebInputBodyFormat::MsgPack,
		_ => WebInputBodyFormat::Json,
	}
}

fn normalize_web_path(p: &str) -> String {
	let p = p.trim();
	let p = if p.starts_with('/') {
		p.to_string()
	} else {
		format!("/{}", p.trim_start_matches('/'))
	};
	let p = p.trim_end_matches('/');
	if p.is_empty() {
		"/".to_string()
	} else {
		p.to_string()
	}
}

impl fmt::Display for WebInputMethod {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Get => write!(f, "GET"),
			Self::Post => write!(f, "POST"),
		}
	}
}

/// `App` に `web::Data::<Arc<WebInputRegistry>>` と `web::Data::<SharedState>` を登録したうえで呼ぶ。
pub fn register_web_input_routes(cfg: &mut web::ServiceConfig, registry: &Arc<WebInputRegistry>) {
	for ep in &registry.endpoints {
		let path = ep.path.clone();
		match ep.method {
			WebInputMethod::Get => {
				cfg.service(web::resource(&path).route(web::get().to(get_multiplex)));
			}
			WebInputMethod::Post => {
				cfg.service(web::resource(&path).route(web::post().to(post_multiplex)));
			}
		}
	}
}

pub async fn get_multiplex(
	req: HttpRequest,
	query: web::Query<InputPayload>,
	state: web::Data<SharedState>,
	registry: web::Data<Arc<WebInputRegistry>>,
) -> VacResult<impl Responder> {
	let ep = match registry.resolve(req.path(), WebInputMethod::Get) {
		Some(e) => e,
		None => {
			log::error!("《WebInput》: 未登録のパス {}", req.path());
			return Ok(HttpResponse::InternalServerError().finish());
		}
	};
	handle_input(state.get_ref(), ep, query.into_inner()).await
}

pub async fn post_multiplex(
	req: HttpRequest,
	body: web::Bytes,
	state: web::Data<SharedState>,
	registry: web::Data<Arc<WebInputRegistry>>,
) -> VacResult<impl Responder> {
	let ep = match registry.resolve(req.path(), WebInputMethod::Post) {
		Some(e) => e,
		None => {
			log::error!("《WebInput》: 未登録のパス {}", req.path());
			return Ok(HttpResponse::InternalServerError().finish());
		}
	};

	let payload = match decode_post_body(ep.body_format, &body) {
		Ok(p) => p,
		Err(msg) => {
			#[derive(serde::Serialize)]
			struct ErrBody {
				err: String,
			}
			return Ok(HttpResponse::BadRequest().json(ErrBody { err: msg }));
		}
	};

	handle_input(state.get_ref(), ep, payload).await
}

fn decode_post_body(fmt: WebInputBodyFormat, body: &[u8]) -> std::result::Result<InputPayload, String> {
	if body.is_empty() {
		return Err("empty body".into());
	}
	match fmt {
		WebInputBodyFormat::Json => serde_json::from_slice(body).map_err(|e| e.to_string()),
		WebInputBodyFormat::Toml => {
			let s = std::str::from_utf8(body).map_err(|e| e.to_string())?;
			toml::from_str(s).map_err(|e| e.to_string())
		}
		WebInputBodyFormat::MsgPack => rmp_serde::from_slice(body).map_err(|e| e.to_string()),
	}
}

async fn handle_input(state: &SharedState, ep: &WebInputEndpoint, payload: InputPayload) -> VacResult<HttpResponse> {
	let channel = match (&ep.fixed_channel, &payload.channel) {
		(Some(fixed), _) => fixed.clone(),
		(None, Some(c)) if !c.is_empty() => c.clone(),
		_ => {
			return Ok(HttpResponse::BadRequest()
				.content_type(CONTENT_TYPE_APPLICATION_JSON)
				.body(r#"{"error":"channel is required for this endpoint"}"#));
		}
	};

	if let WebInputCommandOutcome::Handled(response) = process_command(state, &payload).await? {
		return Ok(response);
	}

	let cd = ChannelDatum::new(channel.clone(), payload.content.clone()).with_flag_if(ChannelDatum::FLAG_IS_FINAL, payload.is_final);
	log::trace!("《WebInput》: ChannelDatum を生成しました: {:?}", cd);
	state.read().await.push_channel_datum(cd).await;

	let response = InputPayload {
		channel: Some(channel),
		content: payload.content,
		is_final: payload.is_final,
	};
	Ok(HttpResponse::Ok().content_type(CONTENT_TYPE_APPLICATION_JSON).json(response))
}

async fn process_command(state: &SharedState, payload: &InputPayload) -> VacResult<WebInputCommandOutcome> {
	if payload.content.starts_with("/quit") {
		let broker = {
			let s = state.read().await;
			s.shutdown.clone()
		};
		log::info!("《WebInput》 終了コマンド /quit を受け取りました。graceful shutdown を要求します。");
		broker.trigger(ShutdownReason::ControlApi);
		Ok(WebInputCommandOutcome::Handled(
			HttpResponse::Accepted()
				.content_type(CONTENT_TYPE_APPLICATION_JSON)
				.body(r#"{"status":"shutting_down"}"#),
		))
	} else if payload.content.starts_with("/save") {
		state.read().await.save().await?;
		Ok(WebInputCommandOutcome::Continue)
	} else if payload.content.starts_with("/load") {
		state.write().await.load().await?;
		Ok(WebInputCommandOutcome::Continue)
	} else {
		Ok(WebInputCommandOutcome::Continue)
	}
}
