//! 常駐 AI サービス本体。
//!
//! 旧 `src/processor/openai_chat/mod.rs` の `OpenAiChat` プロセッサーを Phase I で「1 ペルソナ = 1 常駐タスク」の
//! [`AiService`] に昇格させた。プロセッサーの `is_channel_from` + `process` による線形パイプライン駆動ではなく、
//! [`crate::state::State`] から `broadcast` 経由で流れてくる [`Observation`] を event loop で受け取り、
//! `observe.triggers` にマッチしたときだけ応答を評価する。
//!
//! Phase II 以降で加える Decision Engine・Heartbeat・Action 実行はこの event loop を拡張する形で入れる。

use super::completion;
use super::config::{AiPersonaConf, OpenAiChatFinetuning};
use super::context;
use super::decision::{uniform_jitter, Decision, DecisionInput, DecisionSpec};
use super::model_policy;
use super::observe::{Observation, ObserveSet};
use super::tools::{self, ToolContext};
use super::ENV_OPENAI_API_KEY;

use crate::conf::{TwitchEventSubConfig, TwitchModeratorConfig};
use crate::state::AiRuntime;
use crate::{ChannelDatum, SharedChannelData, SharedState};

use anyhow::{anyhow, bail, Result};
use async_openai::{
 config::OpenAIConfig,
 types::chat::{
  ChatCompletionToolChoiceOption, CreateChatCompletionRequest, CreateChatCompletionRequestArgs, ToolChoiceOptions,
 },
 Client,
};
use futures::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tokio::sync::{broadcast, Mutex, RwLock, Semaphore};

const DEFAULT_MEMORY_CAPACITY: usize = 4;
const DEFAULT_REMOVE_CHARS: &str = "\n\r\t";
const DEFAULT_OPENAI_MAX_IN_FLIGHT: usize = 2;
const DEFAULT_OPENAI_STREAM: bool = true;
const DEFAULT_OVERFLOW_SUMMARY_MIN_CHARS: usize = 64;

/// 1 ペルソナ分の AI サービス。`Clone` で同一リソースを共有する（Arc + broadcast）。
///
/// Phase VI-α-4 以降は `persona` / `request_template` / `decision` が
/// **Control API からの hot-reload で atomic に差し替えうる**。そのため外部からこれらを参照する呼び出しは
/// すべて `read().await` を通過させる必要がある。書き換えは [`super::reload::AiReloadHandle`] 経由で行う。
#[derive(Clone)]
pub struct AiService {
 /// ペルソナ設定。hot-reload で `instructions` / `heartbeat.enabled` / `decision.threshold` が変わりうる。
 persona: Arc<RwLock<Arc<AiPersonaConf>>>,
 /// 起動時に固定される表示用ラベル（id ベース）。reload 対象ではないため同期 API に使える。
 persona_label: String,
 channel_utterance: String,
 observe: Arc<ObserveSet>,
 state: SharedState,
 channel_data: SharedChannelData,
 client: Client<OpenAIConfig>,
 /// instructions などに由来する Chat Completion リクエストテンプレート。
 /// `make_request_template()` の結果を atomic swap する。
 request_template: Arc<RwLock<Arc<CreateChatCompletionRequest>>>,
 last_activated: Arc<Mutex<SystemTime>>,
 /// Phase II: 旧 `force_activate_regex` / `ignore_regex` / `min_interval` を包含するスコアリング仕様。
 /// Phase VI-α-4 で reload 対象になったので `RwLock<Arc<_>>`。
 decision: Arc<RwLock<Arc<DecisionSpec>>>,
 in_flight: Arc<Semaphore>,
 overflow_summary_last_at: Arc<Mutex<Option<Instant>>>,
 /// Phase IV: Actions。Twitch 系 Helix ツールに渡す資格情報の所在。未指定時はその系統のツール呼び出しは失敗する。
 twitch_eventsub: Option<Arc<TwitchEventSubConfig>>,
 /// Phase V-c: モデレーション系ツール用のボット/モデレーターアカウント設定。
 twitch_moderator: Option<Arc<TwitchModeratorConfig>>,
 /// `vac_twitch_chat_say` 等で `broadcaster_login` 引数が省略されたときのフォールバック。
 twitch_default_broadcaster_login: Option<String>,
 /// Phase VI-α-2: Control API からの soft pause。`true` の間は新規の `react()` / heartbeat 評価をスキップする。
 /// 既に走っている応答は中断しない（soft suspend）。hard abort はタスクハンドル側から行う別系統。
 paused: Arc<AtomicBool>,
}

impl AiService {
 pub async fn new(
  persona: AiPersonaConf,
  state: SharedState,
  twitch_eventsub: Option<Arc<TwitchEventSubConfig>>,
  twitch_moderator: Option<Arc<TwitchModeratorConfig>>,
  twitch_default_broadcaster_login: Option<String>,
  paused: Arc<AtomicBool>,
 ) -> Result<Self> {
  let channel_utterance = persona
   .channel_utterance
   .clone()
   .ok_or_else(|| anyhow!("AI ペルソナに channel_utterance が設定されていません: id={:?}", persona.id))?;

  if persona.observe.triggers.is_empty() {
   let heartbeat_enabled = persona.heartbeat.as_ref().map(|h| h.enabled).unwrap_or(false);
   if !heartbeat_enabled {
    log::warn!(
     "AI ペルソナ id={:?} は observe.triggers が空で heartbeat も無効です。応答は発生しません。",
     persona.id
    );
   } else {
    log::info!(
     "AI ペルソナ id={:?} は observe.triggers が空ですが heartbeat が有効なので自発評価は走ります。",
     persona.id
    );
   }
  }

  let observe = ObserveSet::from_conf(&channel_utterance, &persona.observe);

  let client = make_client(&persona)?;
  let request_template = make_request_template(&persona)?;

  let decision = Arc::new(DecisionSpec::from_persona(&persona)?);
  log::debug!(
   "《AI[{}]》 Decision engine: {:?} (legacy_compat={})",
   persona.id.as_deref().unwrap_or("-"),
   decision,
   persona.decision.is_none(),
  );

  let max_in_flight = persona
   .openai_max_in_flight
   .unwrap_or(DEFAULT_OPENAI_MAX_IN_FLIGHT)
   .max(1);

  if persona.api_key.is_some() {
   log::warn!("================================================================");
   log::warn!(
    "api_key が設定ファイルで直接設定されています。設定ファイルを共有したり一般に公開する際は不慮の漏出に十分に注意して下さい。または環境変数 {} での設定も検討して下さい。",
    ENV_OPENAI_API_KEY
   );
   log::warn!("================================================================");
  }

  let channel_data = state.read().await.channel_data.clone();

  log::info!(
   "《AI[{}]》 サービスを初期化しました: triggers={:?} channel_utterance={:?} model={:?} include_all={} additional={:?} exclude={:?} max_in_flight={} stream={}",
   persona.id.clone().unwrap_or_else(|| "-".to_string()),
   persona.observe.triggers,
   channel_utterance,
   persona.model,
   persona.observe.include_all,
   persona.observe.include_additional,
   persona.observe.exclude,
   max_in_flight,
   persona.openai_stream.unwrap_or(DEFAULT_OPENAI_STREAM),
  );

  let persona_arc = Arc::new(persona);
  let persona_label = persona_arc
   .id
   .clone()
   .unwrap_or_else(|| "-".to_string());

  Ok(Self {
   persona: Arc::new(RwLock::new(persona_arc)),
   persona_label,
   channel_utterance,
   observe: Arc::new(observe),
   state,
   channel_data,
   client,
   request_template: Arc::new(RwLock::new(Arc::new(request_template))),
   last_activated: Arc::new(Mutex::new(SystemTime::UNIX_EPOCH)),
   decision: Arc::new(RwLock::new(decision)),
   in_flight: Arc::new(Semaphore::new(max_in_flight)),
   overflow_summary_last_at: Arc::new(Mutex::new(None)),
   twitch_eventsub,
   twitch_moderator,
   twitch_default_broadcaster_login,
   paused,
  })
 }

 /// Phase VI-α-2: このサービスが現在 paused かどうか。
 pub fn is_paused(&self) -> bool {
  self.paused.load(Ordering::SeqCst)
 }

 /// Phase IV: function-call 型ツール群に渡す実行コンテキストを生成する。
 ///
 /// `persona` が reload されるため、tool_context も毎回 read lock を通って最新の `channel_effect` を見る。
 async fn tool_context(&self) -> ToolContext {
  let persona = self.persona.read().await.clone();
  ToolContext {
   state: self.state.clone(),
   persona_label: self.persona_label(),
   effect_channel: persona.channel_effect.clone().unwrap_or_else(|| "effect".to_string()),
   twitch_eventsub: self.twitch_eventsub.clone(),
   twitch_moderator: self.twitch_moderator.clone(),
   twitch_default_broadcaster_login: self.twitch_default_broadcaster_login.clone(),
  }
 }

 pub fn persona_label(&self) -> String {
  // Phase VI-α-4: reload 対象でないため、`new()` 時点でキャッシュしたラベルをそのまま返す。
  self.persona_label.clone()
 }

 /// Phase VI-α-4: Control API `/reload` 用の hot-reload ハンドル。
 /// 同じ `Arc<RwLock<Arc<_>>>` を共有しているので、ここで書き換えれば AiService 側にも即座に反映される。
 pub fn reload_handle(&self) -> super::reload::AiReloadHandle {
  super::reload::AiReloadHandle {
   persona_id: self.persona_label_id_opt(),
   persona: self.persona.clone(),
   request_template: self.request_template.clone(),
   decision: self.decision.clone(),
  }
 }

 /// 起動時にキャッシュしたラベルは `"-"` を代入してあるため、id の有無を復元したいときに使う。
 fn persona_label_id_opt(&self) -> Option<String> {
  if self.persona_label == "-" {
   None
  } else {
   Some(self.persona_label.clone())
  }
 }

 /// event loop。`broadcast` からの観測を受け続け、トリガーチャンネルの Datum に対して非同期に応答する。
 pub async fn run(self, mut rx: broadcast::Receiver<Observation>) {
  let label = self.persona_label();
  log::info!("《AI[{}]》 event loop を起動しました。", label);
  loop {
   match rx.recv().await {
    Ok(obs) => {
     if self.is_paused() {
      log::trace!("《AI[{}]》 paused のため観測 {:?} をスキップします。", label, obs);
      continue;
     }
     if !self.observe.is_trigger(&obs.channel) {
      continue;
     }
     let me = self.clone();
     tokio::spawn(async move {
      if let Err(e) = me.react(obs).await {
       log::error!("《AI[{}]》 応答中にエラー: {:?}", me.persona_label(), e);
      }
     });
    },
    Err(broadcast::error::RecvError::Lagged(n)) => {
     log::warn!("《AI[{}]》 観測が {} 件遅延しました（追いつけなかった分は欠落）。", label, n);
    },
    Err(broadcast::error::RecvError::Closed) => {
     log::info!("《AI[{}]》 event loop を終了します（broadcast が閉じられました）。", label);
     break;
    },
   }
  }
 }

 /// Phase III: heartbeat タスク本体。
 ///
 /// `interval_secs` ごとにチャンネルデータを覗き、直近のトリガー発話（`observe.triggers` のいずれか、かつ `is_final`）を
 /// 見つけて合成した [`Observation`] を `react()` に渡して **再評価**する。Decision Engine 側の `SilenceSince`
 /// モジュレータなどによって score が threshold を超えたときだけ実際に応答する。トリガー発話が一つも無いときは
 /// 評価対象が作れないため黙ってスキップする（まっさらな起動直後の暴走を避ける）。
 pub async fn run_heartbeat(self, interval_secs: u64) {
  let label = self.persona_label();
  let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
  tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
  tick.tick().await; // 1 回目の即発を捨てる（起動直後の不要な評価を避ける）
  loop {
   tick.tick().await;

   if self.is_paused() {
    log::trace!("《AI[{}]》 heartbeat: paused のためティックをスキップします。", label);
    continue;
   }

   // Phase VI-α-4: hot-reload で enabled が切り替わりうるため、毎ティック現在値を確認する。
   // 当初は spawn 時に enabled=true だったが reload で false になった、のケースを吸収。
   let hb_enabled_now = self
    .persona
    .read()
    .await
    .heartbeat
    .as_ref()
    .map(|h| h.enabled)
    .unwrap_or(false);
   if !hb_enabled_now {
    log::trace!(
     "《AI[{}]》 heartbeat: enabled=false のためティックをスキップします（reload 反映）。",
     label
    );
    continue;
   }

   let latest = {
    let channel_data = self.channel_data.read().await;
    channel_data
     .iter()
     .rev()
     .find(|cd| cd.has_flag(ChannelDatum::FLAG_IS_FINAL) && self.observe.is_trigger(&cd.channel))
     .map(|cd| (cd.get_id(), cd.channel.clone()))
   };
   let Some((datum_id, channel)) = latest else {
    log::trace!(
     "《AI[{}]》 heartbeat: 評価対象のトリガー発話が見つかりません。スキップ。",
     label
    );
    continue;
   };

   let obs = Observation { datum_id, channel };
   log::trace!("《AI[{}]》 heartbeat: Observation {:?} を再評価します。", label, obs);
   let me = self.clone();
   tokio::spawn(async move {
    if let Err(e) = me.react(obs).await {
     log::error!("《AI[{}]》 heartbeat 応答中にエラー: {:?}", me.persona_label(), e);
    }
   });
  }
 }

 /// 1 回の観測に対する応答生成本体。旧 `OpenAiChat::process` の中身に相当。
 async fn react(&self, obs: Observation) -> Result<()> {
  // Phase VI-α-4: persona / request_template / decision は hot-reload で差し替わるため、
  // react() の開始時にまとめて read lock を取ってスナップショットを作る。
  let persona = self.persona.read().await.clone();
  let observe = self.observe.clone();

  let memory_capacity = persona.memory_capacity.unwrap_or(DEFAULT_MEMORY_CAPACITY);
  let custom_instructions = persona.custom_instructions.clone();
  let system_instructions_extra = persona.system_instructions_extra.clone();
  let memory_summary_path = persona.memory_summary_path.clone();
  let openai_few_shot = persona.openai_few_shot.clone();
  let persona_anchor = persona.persona_anchor.clone();
  let assistant_strip_substrings = persona.assistant_strip_substrings.clone();
  let memory_budget_approx_tokens = persona.memory_budget_approx_tokens;
  let memory_budget_chars_per_approx_token = persona.memory_budget_chars_per_approx_token.unwrap_or(4).max(1);
  let memory_overflow_summary_enabled = persona.memory_overflow_summary_enabled.unwrap_or(false);
  let memory_overflow_summary_model = persona.memory_overflow_summary_model.clone();
  let memory_overflow_summary_max_completion_tokens = persona.memory_overflow_summary_max_completion_tokens;
  let memory_overflow_summary_max_input_chars = persona.memory_overflow_summary_max_input_chars.unwrap_or(16_000);
  let memory_overflow_summary_min_chars = persona.memory_overflow_summary_min_chars.unwrap_or(DEFAULT_OVERFLOW_SUMMARY_MIN_CHARS);
  let memory_overflow_summary_cooldown_secs = persona.memory_overflow_summary_cooldown_secs.filter(|&s| s > 0);
  let model_for_runtime = persona.model.clone();
  let orig_max_tokens = persona.max_tokens;
  let remove_chars = persona
   .remove_chars
   .as_ref()
   .cloned()
   .unwrap_or_else(|| DEFAULT_REMOVE_CHARS.to_string());
  let fine_tuning = persona.fine_tuning.clone();
  let memory_summary = persona.memory_summary.clone();
  let memory_max_chars = persona.memory_max_chars;
  let assistant_max_chars = persona.assistant_max_chars;
  let openai_stream = persona.openai_stream.unwrap_or(DEFAULT_OPENAI_STREAM);
  let openai_tools_json_path = persona.openai_tools_json_path.clone();
  let openai_tool_choice = persona.openai_tool_choice.clone();
  let openai_parallel_tool_calls = persona.openai_parallel_tool_calls;
  let respect_speech_floor = persona.respect_speech_floor.clone();

  let channel_utterance = self.channel_utterance.clone();
  let state = self.state.clone();
  let channel_data = self.channel_data.clone();
  let client = self.client.clone();
  // Arc<CreateChatCompletionRequest> として取得し、後続で `(*request_template).clone()` で深い複製を作る。
  let request_template: Arc<CreateChatCompletionRequest> = self.request_template.read().await.clone();
  let last_activated = self.last_activated.clone();
  let in_flight = self.in_flight.clone();
  let overflow_summary_last_at = self.overflow_summary_last_at.clone();
  let decision: Arc<DecisionSpec> = self.decision.read().await.clone();

  let persona_label = self.persona_label();
  let marker_flag = format!("ai[{}]({}:{})", persona_label, obs.channel, obs.datum_id);

  crate::state::wait_respect_speech_floor_key(&state, respect_speech_floor.as_deref()).await;

  let _in_flight_permit = match in_flight.acquire_owned().await {
   Ok(p) => p,
   Err(_) => {
    log::error!("《AI[{}]》 同時実行セマフォが閉じられました。処理を中断します。", persona_label);
    return Ok(());
   },
  };

  // メモリ窓収集
  let (reversed_sources, dropped_for_overflow) = {
   let channel_data = channel_data.read().await;
   match context::collect_memory_window(&channel_data, obs.datum_id, memory_capacity, &observe) {
    Ok(mut v) => {
     let dropped = if memory_budget_approx_tokens.is_some() {
      context::shrink_memory_window_by_approx_tokens(
       &mut v,
       memory_budget_approx_tokens,
       memory_budget_chars_per_approx_token,
      )
     } else {
      context::shrink_memory_window_by_chars(&mut v, memory_max_chars)
     };
     (v, dropped)
    },
    Err(context::MemoryWindowError::TriggerNotFound) => {
     log::error!(
      "《AI[{}]》 処理対象の入力は既にありませんでした。必要に応じて設定の state_data_capacity を増やすと解決するかもしれません。",
      persona_label
     );
     return Ok(());
    },
    Err(context::MemoryWindowError::TriggerNotFinal) => {
     log::trace!("《AI[{}]》 未確定の入力なので、処理をスキップします。", persona_label);
     return Ok(());
    },
   }
  };

  let overflow_summary_text: Option<String> = if memory_overflow_summary_enabled && !dropped_for_overflow.is_empty() {
   let raw = context::format_dropped_turns_for_overflow_summary(&dropped_for_overflow);
   let truncated = context::truncate_overflow_summary_input(&raw, memory_overflow_summary_max_input_chars);
   let input_chars = truncated.trim().chars().count();
   if input_chars < memory_overflow_summary_min_chars {
    log::debug!(
     "《AI[{}]》 メモリ窓オーバーフロー要約: 入力が短いためスキップ（{} < {} 文字）。",
     persona_label,
     input_chars,
     memory_overflow_summary_min_chars
    );
    None
   } else if let Some(cool_secs) = memory_overflow_summary_cooldown_secs {
    let guard = overflow_summary_last_at.lock().await;
    let skip_for_cooldown = match *guard {
     Some(prev) => prev.elapsed().as_secs() < cool_secs,
     None => false,
    };
    if skip_for_cooldown {
     log::debug!(
      "《AI[{}]》 メモリ窓オーバーフロー要約: クールダウン中のためスキップ（要約 API の最短間隔 {}s）。",
      persona_label,
      cool_secs
     );
     None
    } else {
     drop(guard);
     run_overflow_summary(
      &client,
      &memory_overflow_summary_model,
      model_for_runtime.as_deref(),
      memory_overflow_summary_max_completion_tokens,
      &truncated,
      dropped_for_overflow.len(),
      &overflow_summary_last_at,
     )
     .await
    }
   } else {
    run_overflow_summary(
     &client,
     &memory_overflow_summary_model,
     model_for_runtime.as_deref(),
     memory_overflow_summary_max_completion_tokens,
     &truncated,
     dropped_for_overflow.len(),
     &overflow_summary_last_at,
    )
    .await
   }
  } else {
   None
  };

  let Some(latest_user_content) = reversed_sources
   .iter()
   .find(|cd| observe.is_trigger(&cd.channel))
   .map(|v| v.content.clone())
  else {
   log::warn!(
    "《AI[{}]》 メモリ窓にトリガー（{:?}）の発話がありません。応答をスキップします。",
    persona_label,
    persona.observe.triggers
   );
   return Ok(());
  };

  log::trace!("《AI[{}]》 reversed_sources = {:?}", persona_label, reversed_sources);

  // Phase II: Decision Engine による応答判定。legacy の force/ignore/min_interval も内部で同じ数式に畳んでいる。
  let newest_trimmed = reversed_sources.iter().next().map(|cd| cd.content.trim()).unwrap_or("");
  let last_activated_snapshot = *last_activated.lock().await;
  let decision_result = decision.evaluate(
   &DecisionInput {
    channel: &obs.channel,
    latest_trigger_content: newest_trimmed,
   },
   last_activated_snapshot,
   uniform_jitter,
  );
  match decision_result {
   Decision::Activate { score, threshold } => {
    log::debug!(
     "《AI[{}]》 Decision: Activate (score={:.2} >= threshold={:.2}) target={:?}",
     persona_label,
     score,
     threshold,
     newest_trimmed
    );
    if decision.min_interval_in_secs().is_some() {
     *last_activated.lock().await = SystemTime::now();
    }
   },
   Decision::SkipByScore { score, threshold } => {
    log::trace!(
     "《AI[{}]》 Decision: SkipByScore (score={:.2} < threshold={:.2}) target={:?}",
     persona_label,
     score,
     threshold,
     newest_trimmed
    );
    return Ok(());
   },
   Decision::SkipByMinInterval { remaining_secs } => {
    log::trace!(
     "《AI[{}]》 Decision: SkipByMinInterval（残り {}s）。",
     persona_label,
     remaining_secs
    );
    return Ok(());
   },
  }

  let memory_summary_combined: Option<String> = {
   let inline = memory_summary.clone().unwrap_or_default();
   let mut combined = inline;
   if let Some(ref path) = memory_summary_path {
    match tokio::fs::read_to_string(path).await {
     Ok(file_txt) => {
      let ft = file_txt.trim();
      if !ft.is_empty() {
       if combined.trim().is_empty() {
        combined = file_txt;
       } else {
        combined = format!("{}\n\n---\n\n{}", ft, combined.trim());
       }
      }
     },
     Err(e) => log::warn!(
      "《AI[{}]》 memory_summary_path を読めませんでした: {:?} ({})",
      persona_label,
      path,
      e
     ),
    }
   }
   if let Some(ov) = overflow_summary_text {
    let block = format!("【メモリ窓から落ちた発話の要約】\n{}", ov.trim());
    combined = if combined.trim().is_empty() {
     block
    } else {
     format!("{}\n\n---\n\n{}", block, combined.trim())
    };
   }
   let t = combined.trim();
   if t.is_empty() {
    None
   } else {
    Some(combined)
   }
  };

  // Arc 包みから 1 回だけ実体を深くコピーして手元で可変にする。
  let mut request = (*request_template).clone();
  request.messages = context::assemble_openai_chat_messages(
   model_for_runtime.as_deref(),
   custom_instructions.as_deref(),
   system_instructions_extra.as_deref(),
   memory_summary_combined.as_deref(),
   &openai_few_shot,
   persona_anchor.as_deref(),
   &reversed_sources,
   &observe,
  );

  let mut use_tools_roundtrip = false;
  if let Some(ref p) = openai_tools_json_path {
   match tokio::fs::read_to_string(p).await {
    Ok(s) => match tools::parse_tools_json(&s) {
     Ok(v) if !v.is_empty() => {
      request.tools = Some(v);
      request.tool_choice = Some(match openai_tool_choice.as_deref() {
       None => ChatCompletionToolChoiceOption::Mode(ToolChoiceOptions::Auto),
       Some(tc) => match tools::parse_tool_choice(tc) {
        Ok(x) => x,
        Err(e) => {
         log::warn!("openai_tool_choice を解釈できません: {} — auto にします。", e);
         ChatCompletionToolChoiceOption::Mode(ToolChoiceOptions::Auto)
        },
       },
      });
      if let Some(ptc) = openai_parallel_tool_calls {
       request.parallel_tool_calls = Some(ptc);
      }
      use_tools_roundtrip = true;
      log::info!(
       "《AI[{}]》 OpenAI tools を読み込みました: {:?} ({} 件)",
       persona_label,
       p,
       request.tools.as_ref().map(|t| t.len()).unwrap_or(0)
      );
     },
     Ok(_) => log::warn!("《AI[{}]》 openai_tools_json_path の tools が空です: {:?}", persona_label, p),
     Err(e) => log::warn!("《AI[{}]》 openai_tools_json をパースできません: {:?} ({})", persona_label, p, e),
    },
    Err(e) => log::warn!("《AI[{}]》 openai_tools_json_path を読めませんでした: {:?} ({})", persona_label, p, e),
   }
  }

  let effective_stream = openai_stream && !use_tools_roundtrip;
  if openai_stream && use_tools_roundtrip {
   log::warn!(
    "《AI[{}]》 openai_tools_json_path が指定されているため、ツール解決のためストリーミングをオフにします。",
    persona_label
   );
  }

  log::debug!("《AI[{}]》 応答をリクエストします（stream={}）。", persona_label, effective_stream);

  let mut content = String::new();
  let mut stream_datum_id: Option<u64> = None;
  let mut stream_nonempty_delta_chunks: usize = 0;

  if effective_stream {
   request.stream = Some(true);
   request.n = Some(1);
   let mut stream = match client.chat().create_stream(request).await {
    Ok(s) => s,
    Err(e) => {
     log::error!("《AI[{}]》 ストリーミングの開始に失敗しました: {:?}", persona_label, e);
     eprint_openai_usage_hint_if_needed(&e.to_string());
     bail!("{e:?}");
    },
   };
   while let Some(item) = stream.next().await {
    let chunk = item.map_err(|e| anyhow!("{e:?}"))?;
    for choice in chunk.choices {
     if let Some(delta) = choice.delta.content {
      if delta.is_empty() {
       continue;
      }
      stream_nonempty_delta_chunks += 1;
      content.push_str(&delta);
      match stream_datum_id {
       None => {
        let cd = ChannelDatum::new(channel_utterance.clone(), content.clone());
        let sid = cd.get_id();
        state.read().await.push_channel_datum_quiet(cd).await;
        stream_datum_id = Some(sid);
       },
       Some(sid) => {
        state.read().await.update_channel_datum_content_by_id(sid, content.clone()).await;
       },
      }
     }
    }
   }
   log::trace!(
    "《AI[{}]》 SSE 終了: nonempty_delta_chunks={} assistant_chars={}",
    persona_label,
    stream_nonempty_delta_chunks,
    content.chars().count(),
   );
  } else if use_tools_roundtrip {
   let tool_ctx = self.tool_context().await;
   content = match completion::create_chat_completion_resolve_tools(&client, request, &tool_ctx).await {
    Ok(c) => c,
    Err(e) => {
     log::error!("《AI[{}]》 ツール解決のリクエストに失敗しました: {:?}", persona_label, e);
     eprint_openai_usage_hint_if_needed(&e.to_string());
     bail!("{e:?}");
    },
   };
  } else {
   let response = match completion::create_chat_completion(&client, request.clone()).await {
    Ok(response) => response,
    Err(e) => {
     log::error!("《AI[{}]》 OpenAI へのリクエストに失敗しました: {:?}", persona_label, e);
     eprint_openai_usage_hint_if_needed(&e.to_string());
     bail!("{e:?}");
    },
   };
   log::trace!("《AI[{}]》 response = {:?}", persona_label, response);
   content = completion::extract_assistant_text(&response)?;
  }

  if model_policy::should_retry_on_empty_assistant(model_for_runtime.as_deref()) && content.trim().is_empty() {
   log::warn!("《AI[{}]》 gpt-5 系モデルから空の content が返却されました。再試行します。", persona_label);
   if let Some(model) = model_for_runtime.as_deref() {
    if let Some(filled) = completion::retry_if_gpt5_empty_content(&client, model, &latest_user_content, orig_max_tokens).await
    {
     content = filled;
    }
   }
  }

  content = apply_openai_output_filters(content, &remove_chars, assistant_max_chars, &assistant_strip_substrings);

  if content.trim().is_empty() {
   log::warn!(
    "《AI[{}]》 最終応答が空です（下流へ渡ります）。model={:?} stream={} sse_nonempty_deltas={}",
    persona_label,
    model_for_runtime.as_deref(),
    effective_stream,
    stream_nonempty_delta_chunks,
   );
  }

  if let Some(ft) = fine_tuning {
   if let Err(e) = append_fine_tuning_record(&ft, &custom_instructions, &latest_user_content, &content).await {
    log::warn!("《AI[{}]》 fine-tuning ログ追記に失敗しました: {:?}", persona_label, e);
   }
  }

  if let Some(sid) = stream_datum_id {
   state.read().await.finalize_channel_datum_and_dispatch(sid, content, &marker_flag).await;
  } else {
   let datum = ChannelDatum::new(channel_utterance, content)
    .with_flag(ChannelDatum::FLAG_IS_FINAL)
    .with_flag(&marker_flag);
   state.read().await.push_channel_datum(datum).await;
  }

  Ok(())
 }
}

/// `[[ai.personas]]` を走査し、有効なペルソナの [`AiService`] を立ち上げる。
/// 各ペルソナについて:
/// - 1 本は `broadcast` を受ける通常の event loop（`AiService::run`）
/// - `heartbeat.enabled = true` ならもう 1 本は heartbeat タスク（`AiService::run_heartbeat`）
pub async fn spawn_all(
 conf: &super::config::AiConf,
 state: SharedState,
 tx: broadcast::Sender<Observation>,
 twitch_eventsub: Option<Arc<TwitchEventSubConfig>>,
 twitch_moderator: Option<Arc<TwitchModeratorConfig>>,
 twitch_default_broadcaster_login: Option<String>,
) -> Result<Vec<tokio::task::JoinHandle<()>>> {
 let mut handles = Vec::new();
 let mut runtimes: Vec<AiRuntime> = Vec::new();
 for p in &conf.personas {
  if !p.is_enabled {
   log::info!("《AI[{}]》 は is_enabled = false のためスキップします。", p.id.clone().unwrap_or_else(|| "-".to_string()));
   continue;
  }
  // Phase VI-α-2: AiService の paused フラグと State 側の AiRuntime を同じ Arc<AtomicBool> で結ぶ。
  // 以降、Control API の /pause・/resume は AiRuntime 側を書き換えるだけで AiService 側にも即座に伝わる。
  let mut runtime = AiRuntime::new(p.id.clone());
  let paused_flag = runtime.paused.clone();
  let svc = AiService::new(
   p.clone(),
   state.clone(),
   twitch_eventsub.clone(),
   twitch_moderator.clone(),
   twitch_default_broadcaster_login.clone(),
   paused_flag,
  )
  .await?;
  // Phase VI-α-4: hot-reload ハンドルを AiRuntime に登録する。以降 Control API の /reload からアクセス可能。
  runtime.reload_handle = Some(svc.reload_handle());

  // Phase VI-α-4: heartbeat はブロック自体が定義されていれば常に spawn しておき、`enabled` は
  // 毎ティック参照するようにした。これにより Control API から reload で enabled を切り替えられる。
  if let Some(ref hb) = p.heartbeat {
   let interval = hb.interval_secs.max(1);
   if hb.enabled {
    log::info!(
     "《AI[{}]》 heartbeat を有効化しました（{} 秒周期）。",
     svc.persona_label(),
     interval
    );
   } else {
    log::info!(
     "《AI[{}]》 heartbeat は enabled=false で待機しています（{} 秒周期、reload で有効化可）。",
     svc.persona_label(),
     interval
    );
   }
   let svc_hb = svc.clone();
   handles.push(tokio::spawn(async move { svc_hb.run_heartbeat(interval).await }));
  }

  let rx = tx.subscribe();
  handles.push(tokio::spawn(async move { svc.run(rx).await }));
  runtimes.push(runtime);
 }
 // 登録済みペルソナの一覧を State に反映（Control API の snapshot / pause / resume が参照する）。
 // State 外側ロックは Arc を取り出すだけで即座に解放し、以降は ai_runtimes 自身の RwLock だけを握る。
 let ar = state.read().await.ai_runtimes.clone();
 ar.write().await.extend(runtimes);
 Ok(handles)
}

async fn run_overflow_summary(
 client: &Client<OpenAIConfig>,
 overflow_model: &Option<String>,
 main_model: Option<&str>,
 max_completion_tokens: Option<u16>,
 truncated: &str,
 dropped_count: usize,
 last_at: &Arc<Mutex<Option<Instant>>>,
) -> Option<String> {
 let model_ov = match overflow_model.as_deref().or(main_model) {
  Some(m) => m,
  None => {
   log::warn!("AI: `model` が未設定のためオーバーフロー要約をスキップします。");
   return None;
  },
 };
 log::debug!(
  "メモリ窓オーバーフロー要約を実行します（落とした発話 {} 件、要約 model={}）。",
  dropped_count,
  model_ov
 );
 {
  let mut g = last_at.lock().await;
  *g = Some(Instant::now());
 }
 match completion::summarize_overflow_turns(client, model_ov, max_completion_tokens, truncated).await {
  Ok(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
  Ok(_) => {
   log::warn!("メモリ窓オーバーフロー要約の結果が空でした。長期メモには載せません。");
   None
  },
  Err(e) => {
   log::warn!("メモリ窓オーバーフロー要約に失敗しました（本応答は続行）: {:?}", e);
   None
  },
 }
}

async fn append_fine_tuning_record(
 fine_tuning: &OpenAiChatFinetuning,
 custom_instructions: &Option<String>,
 latest_user_content: &str,
 assistant_content: &str,
) -> Result<()> {
 use serde::Serialize;
 use tokio::io::AsyncWriteExt;

 #[derive(Serialize)]
 struct Line {
  messages: Vec<Message>,
 }
 #[derive(Serialize)]
 struct Message {
  role: String,
  content: String,
 }

 let train_path = fine_tuning.train_path();

 if train_path.to_lowercase().ends_with(".csv") {
  let f = tokio::fs::OpenOptions::new().create(true).append(true).open(&train_path).await?;
  let is_new_file = f.metadata().await?.len() == 0;
  let mut w = csv_async::AsyncWriter::from_writer(f);
  if is_new_file {
   w.write_record(vec!["user", "assistant"]).await?;
  }
  w.write_record(vec![latest_user_content, assistant_content]).await?;
 } else {
  let mut line = Line { messages: vec![] };
  if let Some(ci) = custom_instructions.as_ref() {
   line.messages.push(Message {
    role: "system".to_string(),
    content: ci.clone(),
   });
  }
  line.messages.push(Message {
   role: "user".to_string(),
   content: latest_user_content.to_string(),
  });
  line.messages.push(Message {
   role: "assistant".to_string(),
   content: assistant_content.to_string(),
  });
  let line = serde_json::to_string(&line)?.replace('\n', "");
  tokio::fs::OpenOptions::new()
   .create(true)
   .append(true)
   .open(&train_path)
   .await?
   .write_all(format!("{}\n", line).as_bytes())
   .await?;
 };

 log::trace!("fine-tuning 用のファイルに追記しました: {:?}", train_path);
 Ok(())
}

fn apply_openai_output_filters(
 mut content: String,
 remove_chars: &str,
 assistant_max_chars: Option<usize>,
 strip_substrings: &[String],
) -> String {
 for sub in strip_substrings {
  if !sub.is_empty() {
   content = content.replace(sub, "");
  }
 }
 for remove_char in remove_chars.chars() {
  content = content.replace(remove_char, "");
 }
 if let Some(max) = assistant_max_chars {
  let n = content.chars().count();
  if n > max {
   content = content.chars().take(max).collect();
  }
 }
 content
}

fn eprint_openai_usage_hint_if_needed(err: &str) {
 let es = err.to_lowercase();
 if es.contains("billing") || es.contains("quota") || es.contains("limit") || es.contains("exceeded") {
  static MSG: &str = r#"
=================================================================
=================================================================
 OpenAI へのリクエストの失敗理由に
  Billing Exceeded Limit Quota
 などのキーワードが含まれています。使用状況やプランを確認して下さい。
 慌てず落ち着いて Usage ページを確認して計画的に人生を楽しみましょう。
 Usage: https://platform.openai.com/account/usage
=================================================================
=================================================================
"#;
  eprint!("{}", MSG);
 }
}

fn make_client(conf: &AiPersonaConf) -> Result<Client<OpenAIConfig>> {
 let api_key = crate::utility::load_from_env_or_conf(ENV_OPENAI_API_KEY, &conf.api_key);
 if let Some(api_key) = api_key {
  Ok(Client::with_config(OpenAIConfig::default().with_api_key(api_key)))
 } else {
  bail!(
   "OpenAI の API KEY が設定されていません。環境変数 VAC_OPENAI_API_KEY を設定するか、`[[ai.personas]]` の api_key を設定して下さい。"
  );
 }
}

fn make_request_template(conf: &AiPersonaConf) -> Result<CreateChatCompletionRequest> {
 let mut builder = CreateChatCompletionRequestArgs::default();

 if let Some(model) = conf.model.as_ref() {
  builder.model(model.clone());
  model_policy::apply_model_chat_options(&mut builder, model, conf.max_tokens);
 }
 if let Some(temperature) = conf.temperature {
  builder.temperature(temperature);
 }
 if let Some(top_p) = conf.top_p {
  builder.top_p(top_p);
 }
 if let Some(n) = conf.n {
  builder.n(n);
 }
 if let Some(presence_penalty) = conf.presence_penalty {
  builder.presence_penalty(presence_penalty);
 }
 if let Some(frequency_penalty) = conf.frequency_penalty {
  builder.frequency_penalty(frequency_penalty);
 }
 if let Some(user) = conf.user.as_ref() {
  builder.user(user.clone());
 }

 Ok(builder.build()?)
}
