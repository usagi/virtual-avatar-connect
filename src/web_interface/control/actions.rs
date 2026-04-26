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

use actix_web::web::{self, Data, Json};
use actix_web::{get, post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::SharedState;

use super::dto;
use super::events::ControlEvent;

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

async fn apply_pause(state: &SharedState, target: &PauseTarget, paused: bool) -> HttpResponse {
	let s = state.read().await;

	let mut ais_affected: Vec<usize> = Vec::new();

	match target {
		PauseTarget::Ais => {
			let guard = s.ai_runtimes.read().await;
			for (i, rt) in guard.iter().enumerate() {
				rt.set_paused(paused);
				ais_affected.push(i);
			}
		}
		PauseTarget::Ai { id, index } => {
			if id.is_none() && index.is_none() {
				return bad_request("ai target requires either `id` or `index`");
			}
			let guard = s.ai_runtimes.read().await;
			let found = resolve_ai_index(&guard, id.as_deref(), *index);
			match found {
				Ok(i) => {
					guard[i].set_paused(paused);
					ais_affected.push(i);
				}
				Err(msg) => return bad_request(&msg),
			}
		}
	}

	log::info!(
		"《ControlAPI》 {} 適用: target={:?} ais={:?}",
		if paused { "pause" } else { "resume" },
		target,
		ais_affected
	);

	// WS 購読者にも通知（GUI の即時反映用）。受信者 0 件は無視。
	let target_tag: &'static str = match target {
		PauseTarget::Ais => "ais",
		PauseTarget::Ai { .. } => "ai",
	};
	let _ = s.control_event_tx.send(ControlEvent::PauseState {
		paused,
		target: target_tag,
		processors_affected: vec![],
		ais_affected: ais_affected.clone(),
	});

	HttpResponse::Ok().json(PauseOutcome { ais_affected, paused })
}

fn resolve_ai_index(runtimes: &[crate::state::AiRuntime], id: Option<&str>, index: Option<usize>) -> Result<usize, String> {
	if let Some(wanted) = id {
		for (i, rt) in runtimes.iter().enumerate() {
			if rt.persona_id.as_deref() == Some(wanted) {
				return Ok(i);
			}
		}
		return Err(format!("ai persona id {:?} は登録されていません。", wanted));
	}
	if let Some(i) = index {
		if i < runtimes.len() {
			return Ok(i);
		}
		return Err(format!("ai index {} は範囲外です（有効: 0..{}）。", i, runtimes.len()));
	}
	Err("ai target requires either `id` or `index`".to_string())
}

fn bad_request(reason: &str) -> HttpResponse {
	HttpResponse::BadRequest()
		.content_type("application/json")
		.body(format!(r#"{{"error":"bad_request","reason":"{}"}}"#, reason.replace('"', "\\\"")))
}

/// scope へ各ハンドラを登録する（`control::register` から呼ばれる想定）。
pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(get_snapshot).service(post_pause).service(post_resume);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::state::AiRuntime;

	fn mkai(id: Option<&str>) -> AiRuntime {
		AiRuntime::new(id.map(|s| s.to_string()))
	}

	#[test]
	fn resolve_ai_by_id() {
		let rts = vec![mkai(Some("kaltsit")), mkai(Some("sesa"))];
		let i = resolve_ai_index(&rts, Some("sesa"), None).unwrap();
		assert_eq!(i, 1);
	}

	#[test]
	fn resolve_ai_unknown_id() {
		let rts = vec![mkai(Some("kaltsit"))];
		let err = resolve_ai_index(&rts, Some("nobody"), None).unwrap_err();
		assert!(err.contains("登録されていません"), "unexpected msg: {err}");
	}

	#[test]
	fn ai_runtime_pause_toggle_is_shared() {
		let rt = mkai(Some("kaltsit"));
		let clone = rt.clone();
		assert!(!rt.is_paused());
		clone.set_paused(true);
		assert!(rt.is_paused());
	}

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
