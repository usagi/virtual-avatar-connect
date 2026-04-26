//! Control API の最小エンドポイント群。Phase VI-α の疎通確認用。
//!
//! - `GET /api/v1/control/ping`: 単純な pong + バージョン + サーバー時刻
//! - `GET /api/v1/control/whoami`: 呼び出し元の peer_addr と loopback 判定、トークン由来の診断
//!
//! JSON の形は [`responses`]。

mod responses;

use actix_web::web::Data;
use actix_web::{get, HttpRequest, HttpResponse, Responder};

use crate::web_interface::control::auth::ControlApiRuntime;

use responses::{Pong, WhoAmI};

#[get("/ping")]
pub async fn ping() -> impl Responder {
	HttpResponse::Ok().json(Pong {
		ok: true,
		service: "virtual-avatar-connect/control-api",
		version: env!("CARGO_PKG_VERSION"),
		now: jiff::Timestamp::now().to_string(),
	})
}

#[get("/whoami")]
pub async fn whoami(req: HttpRequest, runtime: Data<ControlApiRuntime>) -> impl Responder {
	let peer = req.peer_addr().map(|a| a.to_string());
	let is_loopback = req.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false);
	HttpResponse::Ok().json(WhoAmI::from_runtime(peer, is_loopback, runtime.as_ref()))
}
