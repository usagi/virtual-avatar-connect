pub mod input;
pub mod output;
mod ws;

use actix_web::{get, HttpResponse};
pub use ws::{websocket, WebSocketServer, WsServerPayload, WsServerPayloadChannelData, WsServerPayloadChannelDatum};

static FAVICON_ICO: &[u8] = include_bytes!("../../favicon.ico");

#[get("/favicon.ico")]
pub async fn favicon() -> HttpResponse {
 HttpResponse::Ok().content_type("image/x-icon").body(FAVICON_ICO)
}
