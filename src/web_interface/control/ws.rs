//! Control API の WebSocket エンドポイント (`GET /api/v1/control/events`).
//!
//! - 認証: 既存の `control_api_auth` middleware が `Authorization: Bearer ...`
//!   もしくは `?token=...` を検証してから本ハンドラに到達する。ハンドラ側では再チェックしない。
//! - 配信: [`crate::state::State::control_event_tx`] から subscribe した `broadcast::Receiver` を
//!   actix actor の context で回し、届いた [`ControlEvent`] を JSON 化して `ws::text` で流すだけ。
//! - 受信: 現時点ではクライアント → サーバー方向のメッセージは特別な意味を持たない（ping/pong のみ対応）。
//!   将来 `subscribe` / `filter` などのクライアントコマンドを足す余地は残してある。

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use tokio::sync::broadcast;
use tokio::time::Duration;

use super::events::ControlEvent;
use crate::SharedState;

/// キープアライブ送信間隔。ここを短くすると「接続だけしてイベントが来ない」環境でも、
/// プロキシ/ロードバランサーのアイドルタイムアウトを避けやすくなる。
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// 1 本の WS 接続を表す actor。actor の寿命 = 1 接続の寿命。
pub struct ControlEventsWs {
	/// broadcast 受信側。`started()` で `take()` して別 future に渡すため `Option` で持つ。
	rx: Option<broadcast::Receiver<ControlEvent>>,
}

impl ControlEventsWs {
	pub fn new(rx: broadcast::Receiver<ControlEvent>) -> Self {
		Self { rx: Some(rx) }
	}
}

/// actor 内で扱うメッセージ: broadcast 受信タスク → actor の handler 経由で `ws::text` を出すため。
#[derive(Message)]
#[rtype(result = "()")]
struct WsText(pub String);

impl Handler<WsText> for ControlEventsWs {
	type Result = ();
	fn handle(&mut self, msg: WsText, ctx: &mut Self::Context) {
		ctx.text(msg.0);
	}
}

impl Actor for ControlEventsWs {
	type Context = ws::WebsocketContext<Self>;

	fn started(&mut self, ctx: &mut Self::Context) {
		log::debug!("【ControlAPI】 WS events 接続開始");
		// broadcast 購読ループ
		let rx_opt = self.rx.take();
		let addr = ctx.address();
		if let Some(mut rx) = rx_opt {
			ctx.spawn(actix::fut::wrap_future(async move {
				loop {
					match rx.recv().await {
						Ok(ev) => {
							// serde_json::to_string の失敗は型的にあり得ないが、panic させず log に留める
							match serde_json::to_string(&ev) {
								Ok(json) => {
									if addr.send(WsText(json)).await.is_err() {
										// actor が既に stop 済みなら送信が失敗する → 配信ループを終える
										break;
									}
								}
								Err(e) => log::warn!("【ControlAPI】イベント JSON シリアライズ失敗: {e}"),
							}
						}
						Err(broadcast::error::RecvError::Lagged(n)) => {
							// 受信者が遅延して取りこぼした。件数を通知して継続。
							let ev = ControlEvent::Lagged { dropped: n };
							if let Ok(json) = serde_json::to_string(&ev) {
								let _ = addr.send(WsText(json)).await;
							}
						}
						Err(broadcast::error::RecvError::Closed) => {
							log::debug!("【ControlAPI】 control_event_tx が閉じたため WS 配信ループを終了");
							break;
						}
					}
				}
			}));
		}
		// 定期ハートビート送信（GUI 側で「生きていること」を確認したいときに使う）
		let addr2 = ctx.address();
		ctx.spawn(actix::fut::wrap_future(async move {
			loop {
				tokio::time::sleep(HEARTBEAT_INTERVAL).await;
				let ev = ControlEvent::Heartbeat {
					now: jiff::Timestamp::now().to_string(),
				};
				if let Ok(json) = serde_json::to_string(&ev) {
					if addr2.send(WsText(json)).await.is_err() {
						break;
					}
				}
			}
		}));
	}

	fn stopped(&mut self, _ctx: &mut Self::Context) {
		log::debug!("【ControlAPI】 WS events 接続終了");
	}
}

impl StreamHandler<actix_web::Result<ws::Message, ws::ProtocolError>> for ControlEventsWs {
	fn handle(&mut self, msg: actix_web::Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
		match msg {
			Ok(ws::Message::Ping(bin)) => {
				// WebSocket レベルの ping には pong で応答する（keepalive の基本）
				ctx.pong(&bin);
			}
			Ok(ws::Message::Pong(_)) => { /* クライアント側からの pong は黙殺 */ }
			Ok(ws::Message::Text(text)) => {
				// 現状は受信コマンド未定義。明らかに誤用だと判断できるもの以外は黙殺。
				log::trace!("【ControlAPI】 WS events からテキスト受信（未解釈）: {}", text);
			}
			Ok(ws::Message::Binary(_bin)) => {
				log::trace!("【ControlAPI】 WS events からバイナリ受信（未解釈）");
			}
			Ok(ws::Message::Close(close_data)) => {
				log::debug!("【ControlAPI】 WS events クローズ受信: {:?}", close_data);
				ctx.close(close_data);
				ctx.stop();
			}
			Err(e) => {
				log::warn!("【ControlAPI】 WS events プロトコルエラー: {e}");
				ctx.stop();
			}
			_ => {}
		}
	}
}

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
