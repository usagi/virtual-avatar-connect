//! Control API のアクション系エンドポイント（Phase VI-α-2）。
//!
//! - `GET /api/v1/control/snapshot` — 現在の State を read-only DTO で返す。
//! - `POST /api/v1/control/pause`   — 指定 target を soft pause。body に `PauseTarget`。
//! - `POST /api/v1/control/resume`  — 指定 target を解除。
//!
//! δ-9 (v0.9.x) で V1 processor 層を除去したため、`PauseTarget::Processor` / `Processors` は削除。
//! 残っているのは AI persona のみ。Flowgraph ノード単位の pause/resume は将来的にトリガブス経由の
//! 「ノード単位の disable」API として δ-9.x で再整備する予定。
//!
//! pause/resume は Arc<AtomicBool> のみを書き換える軽量操作（soft suspend）。
//! 既に走っている非同期処理は中断しない。
//!
//! pause / resume の適用は [`pause`]。

mod pause;

use actix_web::web::{self, Data, Json};
use actix_web::{get, post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::SharedState;

use super::dto;
use pause::apply_pause;

#[get("/snapshot")]
pub async fn get_snapshot(state: Data<SharedState>) -> impl Responder {
	let s = state.read().await;
	let snap = dto::snapshot(&s).await;
	HttpResponse::Ok().json(snap)
}

/// pause/resume のターゲット指定。v0.9 以降は AI のみ。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum PauseTarget {
	/// 全 AI persona
	Ais,
	/// 特定の AI persona（`AiPersonaConf.id` または登録順 index）
	Ai {
		#[serde(default)]
		id: Option<String>,
		#[serde(default)]
		index: Option<usize>,
	},
}

#[derive(Debug, Serialize)]
pub struct PauseOutcome {
	pub ais_affected: Vec<usize>,
	pub paused: bool,
}

#[post("/pause")]
pub async fn post_pause(state: Data<SharedState>, body: Json<PauseTarget>) -> impl Responder {
	apply_pause(state.get_ref(), &body, true).await
}

#[post("/resume")]
pub async fn post_resume(state: Data<SharedState>, body: Json<PauseTarget>) -> impl Responder {
	apply_pause(state.get_ref(), &body, false).await
}

/// scope へ各ハンドラを登録する（`control::register` から呼ばれる想定）。
pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(get_snapshot).service(post_pause).service(post_resume);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn pause_target_serde_roundtrip_ais() {
		let v: PauseTarget = serde_json::from_str(r#"{"target":"ais"}"#).unwrap();
		assert!(matches!(v, PauseTarget::Ais));
	}

	#[test]
	fn pause_target_serde_ai_with_index() {
		let v: PauseTarget = serde_json::from_str(r#"{"target":"ai","index":2}"#).unwrap();
		match v {
			PauseTarget::Ai { id, index } => {
				assert_eq!(id, None);
				assert_eq!(index, Some(2));
			}
			other => panic!("unexpected: {:?}", other),
		}
	}
}
