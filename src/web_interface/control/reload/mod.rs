//! Control API のホットリロード系エンドポイント (Phase VI-α-4)。
//!
//! `POST /api/v1/control/reload` — AI persona 限定で、指定のフィールドを無停止で差し替える。
//!
//! 対応している scope (v0.9 以降):
//!   - AI persona の `custom_instructions` / `system_instructions_extra`
//!   - AI persona の `heartbeat.enabled`
//!   - AI persona の `decision.threshold`
//!
//! δ-9 (v0.9.x) で V1 processor 層を除去したため、`modify_files` など V1 processor 由来の reload
//! 経路は削除。辞書/正規表現の差し替えは将来 Flowgraph ノード側の property として再実装予定 (δ-9.3)。
//!
//! AI persona 適用ロジックは [`ai_persona`]。

mod ai_persona;

use actix_web::web::{self, Data, Json};
use actix_web::{post, Responder};
use serde::{Deserialize, Serialize};

use crate::ai::{AiReloadReport, AiReloadRequest};
use crate::SharedState;

use ai_persona::handle_ai_reload;

/// reload のターゲット指定と payload。
///
/// JSON 例:
/// ```json
/// {"target":"ai_persona","id":"kaltsit","custom_instructions":"..."}
/// {"target":"ai_persona","id":"kaltsit","heartbeat_enabled":false}
/// {"target":"ai_persona","id":"kaltsit","decision_threshold":0.55}
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum ReloadRequest {
	/// AI persona のランタイム値を差し替える。`id` は `AiPersonaConf.id` 完全一致。
	AiPersona {
		#[serde(default)]
		id: Option<String>,
		#[serde(flatten)]
		changes: AiReloadRequest,
	},
}

#[derive(Debug, Clone, Serialize)]
pub struct ReloadResponse {
	/// 適用対象のカテゴリタグ（WS 通知にも流す）
	pub target: &'static str,
	/// 実際に変化があった項目の要約。`ai_persona` の場合のみ present。
	#[serde(skip_serializing_if = "Option::is_none")]
	pub ai_report: Option<AiReloadReport>,
}

#[post("/reload")]
pub async fn post_reload(state: Data<SharedState>, body: Json<ReloadRequest>) -> impl Responder {
	match body.into_inner() {
		ReloadRequest::AiPersona { id, changes } => handle_ai_reload(state.get_ref(), id.as_deref(), changes).await,
	}
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(post_reload);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reload_request_ai_persona_parses() {
		let json = r#"{"target":"ai_persona","id":"kaltsit","custom_instructions":"hi","decision_threshold":0.5}"#;
		let r: ReloadRequest = serde_json::from_str(json).unwrap();
		match r {
			ReloadRequest::AiPersona { id, changes } => {
				assert_eq!(id.as_deref(), Some("kaltsit"));
				assert_eq!(changes.custom_instructions.as_deref(), Some("hi"));
				assert_eq!(changes.decision_threshold, Some(0.5));
				assert_eq!(changes.heartbeat_enabled, None);
			}
		}
	}

	#[test]
	fn reload_request_rejects_unknown_target() {
		let json = r#"{"target":"wizards"}"#;
		let r = serde_json::from_str::<ReloadRequest>(json);
		assert!(r.is_err(), "unknown target must be rejected: {:?}", r);
	}
}
