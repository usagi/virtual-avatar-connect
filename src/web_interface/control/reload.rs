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

use actix_web::web::{self, Data, Json};
use actix_web::{post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use super::events::ControlEvent;
use crate::ai::{AiReloadReport, AiReloadRequest};
use crate::SharedState;

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

async fn handle_ai_reload(state: &SharedState, id: Option<&str>, changes: AiReloadRequest) -> HttpResponse {
 if changes.is_empty() {
  return bad_request("ai_persona target requires at least one changeable field (custom_instructions / system_instructions_extra / heartbeat_enabled / decision_threshold)");
 }

 // reload handle を取り出す（見つけたら即座にロックを解放する）
 let s = state.read().await;
 let ar = s.ai_runtimes.clone();
 let tx = s.control_event_tx.clone();
 drop(s);

 let handle_opt = {
  let guard = ar.read().await;
  match id {
   Some(wanted) => guard
    .iter()
    .find(|r| r.persona_id.as_deref() == Some(wanted))
    .and_then(|r| r.reload_handle.clone()),
   None => {
    // id 省略時は「対象ペルソナが 1 件しかないなら」OK、複数いたら曖昧なのでエラー
    let candidates: Vec<_> = guard.iter().filter_map(|r| r.reload_handle.clone()).collect();
    match candidates.len() {
     0 => None,
     1 => Some(candidates.into_iter().next().unwrap()),
     _ => {
      return bad_request("ai_persona target has multiple candidates; please specify `id`");
     },
    }
   },
  }
 };

 let Some(handle) = handle_opt else {
  return not_found(&format!(
   "ai persona id={:?} not found (or reload handle not registered)",
   id.unwrap_or("-")
  ));
 };

 match handle.apply(changes).await {
  Ok(report) => {
   log::info!(
    "《ControlAPI/reload》 ai_persona id={:?} 適用: custom_instructions={} system_instructions_extra={} heartbeat_enabled={} decision_threshold={} warnings={:?}",
    handle.persona_id,
    report.custom_instructions_changed,
    report.system_instructions_extra_changed,
    report.heartbeat_enabled_changed,
    report.decision_threshold_changed,
    report.warnings
   );
   let _ = tx.send(ControlEvent::Reloaded {
    target: "ai_persona",
    id: handle.persona_id.clone(),
    detail: serde_json::to_value(&report).unwrap_or(serde_json::Value::Null),
   });
   HttpResponse::Ok().json(ReloadResponse {
    target: "ai_persona",
    ai_report: Some(report),
   })
  },
  Err(e) => {
   log::error!("《ControlAPI/reload》 ai_persona reload 失敗: {e:?}");
   HttpResponse::InternalServerError().json(serde_json::json!({
    "error": "ai_persona_reload_failed",
    "reason": e.to_string(),
   }))
  },
 }
}

fn bad_request(reason: &str) -> HttpResponse {
 HttpResponse::BadRequest()
  .content_type("application/json")
  .body(format!(
   r#"{{"error":"bad_request","reason":"{}"}}"#,
   reason.replace('"', "\\\"")
  ))
}

fn not_found(reason: &str) -> HttpResponse {
 HttpResponse::NotFound()
  .content_type("application/json")
  .body(format!(
   r#"{{"error":"not_found","reason":"{}"}}"#,
   reason.replace('"', "\\\"")
  ))
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
   },
  }
 }

 #[test]
 fn reload_request_rejects_unknown_target() {
  let json = r#"{"target":"wizards"}"#;
  let r = serde_json::from_str::<ReloadRequest>(json);
  assert!(r.is_err(), "unknown target must be rejected: {:?}", r);
 }
}
