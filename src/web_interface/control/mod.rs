//! Control API (Phase VI-α)
//!
//! ブラウザ/GUI から VAC のランタイム状態を読んだり、制御コマンドを送ったりするための REST/WebSocket レイヤ。
//! 既存の `/resources`・`/output` 等とは **別スコープ** (`/api/v1/control/*`, `/ws/control`) に切って、
//! 認証ミドルウェアもこのスコープだけに適用する設計。
//!
//! 設計方針:
//!   - アドレス: 既存の [`crate::conf::Conf::web_ui_address`] を流用（既定 `127.0.0.1:57000`）。
//!     「どこから接続できるか」は bind で決める。
//!   - 認証: [`crate::conf::ControlApiConf`] によるポリシー。
//!     「その接続に Bearer が要るか」をソース IP（loopback / non-loopback）で分岐。
//!   - 既定: 同 PC 無認証、LAN は Bearer 必須。フルトラスト LAN や強めガードなど設定で切り替え可能。
//!
//! 現時点では疎通確認用の `/ping` と `/whoami` のみ。後続 PR で snapshot / pause/resume / events を足す。

pub mod actions;
pub mod auth;
pub mod bos;
pub mod dto;
pub mod events;
pub mod flowgraph;
pub mod ingress;
pub mod managed_app;
pub mod oauth_twitch;
pub mod ping;
pub mod profiles;
pub mod reload;
pub mod restart;
pub mod run_with;
pub mod shutdown;
pub mod table;
pub mod ws;

pub use auth::{ControlApiRuntime, TokenSource};

use actix_web::web;

/// actix-web のルーターに Control API のルート群を登録する。
///
/// `app_data` に [`ControlApiRuntime`] と [`crate::SharedState`] が挿入されている前提。
/// `lib.rs` の `run_services` から呼ばれる。
pub fn register(cfg: &mut web::ServiceConfig) {
	cfg.service(
		web::scope("/api/v1/control")
			.wrap(actix_web::middleware::from_fn(auth::control_api_auth))
			.service(ping::ping)
			.service(ping::whoami)
			.service(ws::events_ws)
			.configure(actions::configure)
			.configure(reload::configure)
			.configure(restart::configure)
			.configure(shutdown::configure)
			.configure(profiles::configure)
			.configure(run_with::configure)
			.configure(bos::configure)
			.configure(managed_app::configure)
			.configure(oauth_twitch::configure)
			.configure(ingress::configure)
			.configure(flowgraph::configure)
			.configure(table::configure),
	);
}
