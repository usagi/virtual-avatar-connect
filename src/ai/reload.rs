//! Phase VI-α-4: Control API から AI ペルソナの一部設定を hot-reload するためのハンドル。
//!
//! 対応範囲は「gradual enablement」の既定値:
//!   - `custom_instructions` / `system_instructions_extra` — system prompt の差し替え
//!     （`context::assemble_openai_chat_messages` が毎回 persona から読むので rebuild 不要）
//!   - `heartbeat.enabled` — heartbeat の有効/無効（run_heartbeat は毎ティック参照）
//!   - `decision.threshold` — 応答可否のしきい値（`DecisionSpec` が再構築される）
//!
//! それ以外のフィールド（`model`, `tools`, `memory_capacity`, `channel_utterance` など）は
//! このハンドルでは触らない。変えたい場合はプロセス再起動が必要。
//!
//! 設計:
//!   - `AiService` 側は `persona / request_template / decision` を
//!     `Arc<RwLock<Arc<T>>>` として保持している。本ハンドルは同じ `Arc` を共有しているため、
//!     書き換えは AiService 側から即座に観測できる。
//!   - `AiReloadHandle` は [`crate::state::AiRuntime`] に埋め込まれ、Control API の
//!     `/reload` エンドポイントからアクセスされる。cycle 回避のため AiService 全体ではなく
//!     mutable な 3 ハンドルだけを持つ（`state: SharedState` を含まない）。

use anyhow::Result;
use async_openai::types::chat::CreateChatCompletionRequest;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::config::AiPersonaConf;
use super::decision::DecisionSpec;
use super::model_policy;
use super::openai_responses::types::request::{CreateResponseRequest, ReasoningEffort};

// NOTE:
//   `CreateChatCompletionRequest` は他フィールドの reload 対応を追加した際の rebuild 先として
//   ハンドル側に枠だけ残してある（現状の instructions 経路では `react()` 内で毎回持ち込むので rebuild 不要）。
//   将来 `model` / `tools` / `max_tokens` を reload 対象に入れるときはここで rebuild して swap する。

/// Control API から飛んでくる reload 要求。未指定フィールドは現状維持。
///
/// JSON 例:
/// ```json
/// { "custom_instructions": "お前はクールで寡黙な医師だ" }
/// { "heartbeat_enabled": false }
/// { "decision_threshold": 0.6 }
/// { "custom_instructions": "" }              // 空文字列で「instructions 無し」に設定
/// ```
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AiReloadRequest {
 /// 新しい custom instructions 本文（= system prompt の主要部分）。
 /// 空文字列 `""` は「instructions 無し」を意味し、`None` の区別は「現状維持」。
 #[serde(default)]
 pub custom_instructions: Option<String>,
 /// 新しい system_instructions_extra 本文。挙動は custom_instructions と同じ。
 #[serde(default)]
 pub system_instructions_extra: Option<String>,
 /// `heartbeat.enabled` を切り替える（`heartbeat` ブロック自体が persona 側に存在する前提）。
 #[serde(default)]
 pub heartbeat_enabled: Option<bool>,
 /// `decision.threshold` を上書きする。
 #[serde(default)]
 pub decision_threshold: Option<f64>,
}

impl AiReloadRequest {
 /// 何も指定されていないか（= no-op）。
 pub fn is_empty(&self) -> bool {
  self.custom_instructions.is_none()
   && self.system_instructions_extra.is_none()
   && self.heartbeat_enabled.is_none()
   && self.decision_threshold.is_none()
 }
}

/// reload 適用結果の概要。各 `*_changed` は「実際に差分があった」ときに true。
#[derive(Debug, Clone, Default, Serialize)]
pub struct AiReloadReport {
 pub custom_instructions_changed: bool,
 pub system_instructions_extra_changed: bool,
 pub heartbeat_enabled_changed: bool,
 pub decision_threshold_changed: bool,
 /// reload の副作用として rebuild した派生物（デバッグ用）。
 pub rebuilt_request_template: bool,
 pub rebuilt_decision_spec: bool,
 /// 警告メッセージ（無視された指定など）。
 #[serde(default)]
 pub warnings: Vec<String>,
}

/// 1 ペルソナ分の hot-reload ハンドル。
///
/// `AiService` と同じ `Arc<RwLock<Arc<_>>>` を共有しているので、書き換えは即座に AiService 側にも反映される。
#[derive(Clone)]
pub struct AiReloadHandle {
 pub persona_id: Option<String>,
 pub persona: Arc<RwLock<Arc<AiPersonaConf>>>,
 pub request_template: Arc<RwLock<Arc<CreateChatCompletionRequest>>>,
 pub decision: Arc<RwLock<Arc<DecisionSpec>>>,
}

impl std::fmt::Debug for AiReloadHandle {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  f.debug_struct("AiReloadHandle")
   .field("persona_id", &self.persona_id)
   .finish_non_exhaustive()
 }
}

impl AiReloadHandle {
 /// 指定された変更を適用する。派生物の rebuild は **差分がある場合のみ**。
 ///
 /// `make_request_template` / `DecisionSpec::from_persona` が失敗した場合、書き込み途中で中断して
 /// 元の値を保った `Err` を返す（atomic に近い保証は無いが、reload は manual trigger なので許容）。
 pub async fn apply(&self, req: AiReloadRequest) -> Result<AiReloadReport> {
  if req.is_empty() {
   return Ok(AiReloadReport::default());
  }

  let mut report = AiReloadReport::default();

  // 1) persona をクローンして変更後のスナップショットを作る
  let current: Arc<AiPersonaConf> = self.persona.read().await.clone();
  let mut next: AiPersonaConf = (*current).clone();

  if let Some(ins) = req.custom_instructions.clone() {
   let new_value = if ins.is_empty() { None } else { Some(ins) };
   if new_value != next.custom_instructions {
    next.custom_instructions = new_value;
    report.custom_instructions_changed = true;
   }
  }
  if let Some(ins) = req.system_instructions_extra.clone() {
   let new_value = if ins.is_empty() { None } else { Some(ins) };
   if new_value != next.system_instructions_extra {
    next.system_instructions_extra = new_value;
    report.system_instructions_extra_changed = true;
   }
  }

  if let Some(en) = req.heartbeat_enabled {
   match next.heartbeat.as_mut() {
    Some(hb) => {
     if hb.enabled != en {
      hb.enabled = en;
      report.heartbeat_enabled_changed = true;
     }
    },
    None => {
     report.warnings.push(
      "heartbeat ブロックがペルソナ設定に無いため `heartbeat_enabled` は無視されました（再起動が必要です）".to_string(),
     );
    },
   }
  }

  if let Some(th) = req.decision_threshold {
   match next.decision.as_mut() {
    Some(dec) => {
     if (dec.threshold - th).abs() > f64::EPSILON {
      dec.threshold = th;
      report.decision_threshold_changed = true;
     }
    },
    None => {
     report.warnings.push(
      "decision ブロックがペルソナ設定に無いため `decision_threshold` は無視されました（再起動が必要です）".to_string(),
     );
    },
   }
  }

  // 何も変わらなかったら早期 return（書き込みロック不要）。
  let any_changed = report.custom_instructions_changed
   || report.system_instructions_extra_changed
   || report.heartbeat_enabled_changed
   || report.decision_threshold_changed;
  if !any_changed {
   return Ok(report);
  }

  let next_arc = Arc::new(next);

  // 2) 派生物を rebuild（instructions は毎回 persona から読まれるので rebuild 不要）
  let new_decision = if report.decision_threshold_changed {
   let d = DecisionSpec::from_persona(&next_arc)?;
   report.rebuilt_decision_spec = true;
   Some(d)
  } else {
   None
  };

  // 3) atomic swap（持っているハンドル順に書き込む）
  *self.persona.write().await = next_arc;
  if let Some(d) = new_decision {
   *self.decision.write().await = Arc::new(d);
  }

  Ok(report)
 }
}

// ============================================================
// χ-4: Responses API 用の request template builder。
//
// [`service::make_request_template`] の Responses 版。χ-5 で AiService が Responses
// API に全面移行する際、このビルダを使って template を用意する。`input` / `tools` /
// `tool_choice` / `stream` は **per-request** に詰めるので、ここでは静的な persona
// 設定（model / temperature / max_output_tokens / reasoning / store 等）だけ埋める。
//
// NOTE: 現時点では `AiPersonaConf` に `openai_max_output_tokens` / `openai_reasoning_effort`
// フィールドは存在しない（χ-6 で追加）。χ-4 ではビルダのシグネチャに引数として受け取り、
// χ-5 / χ-6 の接続を待つ。
// ============================================================

/// persona から Responses API の request template を組み立てる。
///
/// `openai_max_output_tokens` / `openai_reasoning_effort` は χ-6 で persona に追加される
/// 予定の optional フィールド。それまでは呼び出し側（service）で明示 None を渡す。
pub(crate) fn make_responses_request_template(
 conf: &AiPersonaConf,
 openai_max_output_tokens: Option<u32>,
 openai_reasoning_effort: Option<ReasoningEffort>,
 openai_store: Option<bool>,
) -> Result<CreateResponseRequest>
{
 let mut request = CreateResponseRequest::default();

 // model とモデル依存オプション（reasoning / text.format）は model が指定された時だけ触る。
 if let Some(model) = conf.model.as_ref()
 {
  request.model = model.clone();
  model_policy::apply_model_responses_options(&mut request, model, openai_max_output_tokens, openai_reasoning_effort);
 }
 else if let Some(mt) = openai_max_output_tokens
 {
  // model 未指定でも `openai_max_output_tokens` は尊重する（model_policy は通らないので直接代入）。
  request.max_output_tokens = Some(mt);
 }

 // 旧 `max_tokens`（u16, Chat Completions 由来）経由のレガシー構成でも χ-6 の切り替え前に
 // fallback を効かせる。明示の `openai_max_output_tokens` が指定されなかった場合に限り
 // `max_tokens` を Responses の `max_output_tokens` に昇格させる。χ-6 で conf の一本化が
 // 済んだらこの分岐は削除。
 if request.max_output_tokens.is_none()
 {
  if let Some(mt) = conf.max_tokens
  {
   request.max_output_tokens = Some(u32::from(mt));
  }
 }

 if let Some(temperature) = conf.temperature
 {
  request.temperature = Some(temperature);
 }
 if let Some(top_p) = conf.top_p
 {
  request.top_p = Some(top_p);
 }
 if let Some(store) = openai_store
 {
  request.store = Some(store);
 }

 Ok(request)
}

#[cfg(test)]
mod responses_template_tests
{
 use super::*;
 use crate::ai::openai_responses::types::request::TextFormat;

 fn persona_minimal() -> AiPersonaConf
 {
  AiPersonaConf {
   id: Some("test".to_string()),
   is_enabled: true,
   channel_utterance: Some("to".to_string()),
   channel_effect: None,
   observe: Default::default(),
   decision: None,
   heartbeat: None,
   api_key: None,
   model: Some("gpt-5-mini".to_string()),
   custom_instructions: None,
   system_instructions_extra: None,
   max_tokens: None,
   temperature: Some(0.5),
   top_p: Some(0.9),
   n: None,
   presence_penalty: None,
   frequency_penalty: None,
   user: None,
   memory_capacity: None,
   memory_max_chars: None,
   memory_budget_approx_tokens: None,
   memory_budget_chars_per_approx_token: None,
   memory_overflow_summary_enabled: None,
   memory_overflow_summary_model: None,
   memory_overflow_summary_max_completion_tokens: None,
   memory_overflow_summary_max_input_chars: None,
   memory_overflow_summary_min_chars: None,
   memory_overflow_summary_cooldown_secs: None,
   memory_summary: None,
   memory_summary_path: None,
   openai_few_shot: Vec::new(),
   persona_anchor: None,
   force_activate_regex_pattern: None,
   ignore_regex_pattern: None,
   min_interval_in_secs: None,
   remove_chars: None,
   assistant_max_chars: None,
   assistant_strip_substrings: Vec::new(),
   openai_stream: None,
   openai_tools_json_path: None,
   openai_tool_choice: None,
   openai_parallel_tool_calls: None,
   openai_max_in_flight: None,
   fine_tuning: None,
   respect_speech_floor: None,
  }
 }

 #[test]
 fn make_responses_template_uses_model_and_temperature()
 {
  let persona = persona_minimal();
  let req = make_responses_request_template(&persona, Some(512), Some(ReasoningEffort::Medium), Some(false)).unwrap();
  assert_eq!(req.model, "gpt-5-mini");
  assert_eq!(req.temperature, Some(0.5));
  assert_eq!(req.top_p, Some(0.9));
  assert_eq!(req.max_output_tokens, Some(512));
  assert_eq!(req.store, Some(false));
  assert_eq!(req.reasoning.as_ref().and_then(|r| r.effort), Some(ReasoningEffort::Medium));
  assert!(matches!(
   req.text.as_ref().and_then(|t| t.format.clone()),
   Some(TextFormat::Text)
  ));
 }

 #[test]
 fn make_responses_template_falls_back_to_legacy_max_tokens()
 {
  let mut persona = persona_minimal();
  persona.max_tokens = Some(256);
  let req = make_responses_request_template(&persona, None, None, None).unwrap();
  assert_eq!(req.max_output_tokens, Some(256));
 }

 #[test]
 fn make_responses_template_prefers_explicit_max_output_tokens_over_legacy()
 {
  let mut persona = persona_minimal();
  persona.max_tokens = Some(256);
  let req = make_responses_request_template(&persona, Some(777), None, None).unwrap();
  assert_eq!(req.max_output_tokens, Some(777));
 }

 #[test]
 fn make_responses_template_skips_reasoning_for_non_gpt5()
 {
  let mut persona = persona_minimal();
  persona.model = Some("gpt-4o-mini".to_string());
  let req = make_responses_request_template(&persona, None, Some(ReasoningEffort::High), None).unwrap();
  assert!(req.reasoning.is_none());
  assert!(req.text.is_none());
 }

 #[test]
 fn make_responses_template_handles_missing_model()
 {
  let mut persona = persona_minimal();
  persona.model = None;
  let req = make_responses_request_template(&persona, Some(100), Some(ReasoningEffort::Low), None).unwrap();
  assert_eq!(req.model, "");
  assert_eq!(req.max_output_tokens, Some(100));
  assert!(req.reasoning.is_none(), "model 未指定時は reasoning も未指定にする");
 }
}
