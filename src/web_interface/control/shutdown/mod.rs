//! Phase ε-1: 穏やかな終了 API（`POST /api/v1/control/shutdown`）。
//!
//! ### 設計の狙い
//!
//! これまで GUI からは `/restart` しかなく、「VAC を終わらせたい」場合はターミナルで Ctrl+C を
//! 2 回叩くしかなかった。2 回目は Windows 上で強制終了となり、終了コードが
//! `STATUS_CONTROL_C_EXIT (0xc000013a)` になるなど、終了ログが汚れる問題があった。
//!
//! 本 API は `State.shutdown`（[`crate::shutdown::ShutdownBroker`]）に `ControlApi` reason で
//! 停止要求を投げるだけの薄いラッパー。broker 受信後の停止手順は `AppCore::run()` 末尾の
//! cleanup フローに統一されており、ここでは「要求を出して 202 で返す」だけで十分。
//!
//! ### restart との違い
//!
//! - `/restart` は新プロセスを spawn してから現プロセスを `process::exit(0)` で殺す。
//!   WS の自動再接続で GUI は新インスタンスに吸い付く想定。
//! - `/shutdown` は新プロセスを spawn しない。broker 経由で actix を graceful stop し、
//!   ManagedApp / bridges / ai の cleanup を踏んでから自然に `run()` が `Ok(())` で返り、
//!   プロセスは exit code 0 で終わる。GUI 側の WS は切断後は再接続できない。
//!
//! DTO は [`types`]。

mod types;

use actix_web::web::{self, Data, Json};
use actix_web::{post, HttpResponse, Responder};

use crate::shutdown::ShutdownReason;
use crate::SharedState;

use types::{ShutdownRequest, ShutdownResponse};

#[post("/shutdown")]
pub async fn post_shutdown(state: Data<SharedState>, body: Option<Json<ShutdownRequest>>) -> impl Responder {
	let _req = body.map(|b| b.into_inner()).unwrap_or_default();

	let current_pid = std::process::id();

	let broker = {
		let s = state.read().await;
		s.shutdown.clone()
	};

	log::info!("《Shutdown》 Control API からの停止要求を受信しました (current_pid={current_pid})。");
	broker.trigger(ShutdownReason::ControlApi);

	HttpResponse::Accepted().json(ShutdownResponse {
		status: "shutting_down",
		current_pid,
	})
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(post_shutdown);
}
