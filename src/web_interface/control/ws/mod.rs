//! Control API の WebSocket エンドポイント (`GET /api/v1/control/events`).
//!
//! - 認証: 既存の `control_api_auth` middleware が `Authorization: Bearer ...`
//!   もしくは `?token=...` を検証してから本ハンドラに到達する。ハンドラ側では再チェックしない。
//! - 配信: [`crate::state::State::control_event_tx`] から subscribe した `broadcast::Receiver` を
//!   actix actor の context で回し、届いた [`ControlEvent`](crate::web_interface::control::events::ControlEvent) を JSON 化して `ws::text` で流すだけ。
//! - 受信: 現時点ではクライアント → サーバー方向のメッセージは特別な意味を持たない（ping/pong のみ対応）。
//!   将来 `subscribe` / `filter` などのクライアントコマンドを足す余地は残してある。
//!
//! actor 実装は [`actor`]。

mod actor;

use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;

use crate::SharedState;

use actor::ControlEventsWs;

/// `GET /api/v1/control/events`（WebSocket upgrade）。middleware 認証通過後に呼ばれる。
#[actix_web::get("/events")]
pub async fn events_ws(
	r: HttpRequest,
	stream: web::Payload,
	state: web::Data<SharedState>,
) -> actix_web::Result<HttpResponse, actix_web::Error> {
	let rx = {
		let s = state.read().await;
		s.control_event_tx.subscribe()
	};
	ws::start(ControlEventsWs::new(rx), &r, stream)
}
