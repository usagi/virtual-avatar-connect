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
