//! Control API の WebSocket (`/api/v1/control/events`) で使うイベント型の **再エクスポート**。
//!
//! 型の定義本体は [`crate::control_events`]（`state` が `web_interface` に依存しないための配置）。

pub use crate::control_events::{ChannelDatumPhase, ControlEvent, ProcessorInvocationOutcome};

use actix_web::web::{Data, Query};
use actix_web::{get, HttpResponse, Responder};
use serde::Deserialize;

use crate::SharedState;

#[derive(Debug, Deserialize)]
pub struct EventHistoryQuery {
	#[serde(default = "default_limit")]
	pub limit: usize,
	pub kind: Option<String>,
}

fn default_limit() -> usize {
	100
}

#[get("/events/history")]
pub async fn history(state: Data<SharedState>, query: Query<EventHistoryQuery>) -> impl Responder {
	let limit = query.limit.clamp(1, 500);
	let kind = query.kind.as_deref().map(str::trim).filter(|s| !s.is_empty());
	let history = {
		let s = state.read().await;
		s.control_event_history.clone()
	};
	let h = history.read().await;
	let mut rows: Vec<_> = h
		.iter()
		.rev()
		.filter(|item| kind.map(|k| item.event.kind() == k).unwrap_or(true))
		.take(limit)
		.cloned()
		.collect();
	rows.reverse();
	HttpResponse::Ok().json(serde_json::json!({
		"events": rows,
		"limit": limit,
	}))
}
