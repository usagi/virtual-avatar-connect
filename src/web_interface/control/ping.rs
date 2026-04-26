//! Control API の最小エンドポイント群。Phase VI-α の疎通確認用。
//!
//! - `GET /api/v1/control/ping`: 単純な pong + バージョン + サーバー時刻
//! - `GET /api/v1/control/whoami`: 呼び出し元の peer_addr と loopback 判定、トークン由来の診断

use actix_web::web::Data;
use actix_web::{get, HttpRequest, HttpResponse, Responder};
use serde::Serialize;

use super::auth::{ControlApiRuntime, TokenSource};

#[derive(Serialize)]
struct Pong {
	ok: bool,
	service: &'static str,
	version: &'static str,
	now: String,
}

#[get("/ping")]
pub async fn ping() -> impl Responder {
	HttpResponse::Ok().json(Pong {
		ok: true,
		service: "virtual-avatar-connect/control-api",
		version: env!("CARGO_PKG_VERSION"),
		now: jiff::Timestamp::now().to_string(),
	})
}

#[derive(Serialize)]
struct WhoAmI {
	peer_addr: Option<String>,
	is_loopback: bool,
	required_token: bool,
	token_source: &'static str,
	token_file: Option<String>,
}

#[get("/whoami")]
pub async fn whoami(req: HttpRequest, runtime: Data<ControlApiRuntime>) -> impl Responder {
	let peer = req.peer_addr().map(|a| a.to_string());
	let is_loopback = req.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false);
	HttpResponse::Ok().json(WhoAmI {
		peer_addr: peer,
		is_loopback,
		required_token: runtime.require_token_for(is_loopback),
		token_source: match runtime.token_source {
			TokenSource::Env => "env",
			TokenSource::Config => "config",
			TokenSource::Generated => "generated",
		},
		token_file: runtime.written_token_file.as_ref().map(|p| p.display().to_string()),
	})
}
