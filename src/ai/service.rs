//! 常駐 AI サービス本体。
//!
//! 旧 `src/processor/openai_chat/mod.rs` の `OpenAiChat` プロセッサーを Phase I で「1 ペルソナ = 1 常駐タスク」の
//! [`AiService`] に昇格させた。プロセッサーの `is_channel_from` + `process` による線形パイプライン駆動ではなく、
//! [`crate::state::State`] から `broadcast` 経由で流れてくる [`Observation`] を event loop で受け取り、
//! `observe.triggers` にマッチしたときだけ応答を評価する。
//!
//! Phase II 以降で加える Decision Engine・Heartbeat・Action 実行はこの event loop を拡張する形で入れる。

use super::completion;
use super::config::{AiPersonaConf, OpenAiChatFinetuning, OpenAiReasoningEffortConf};
use super::context;
use super::decision::{uniform_jitter, Decision, DecisionInput, DecisionSpec};
use super::model_policy;
use super::observe::{Observation, ObserveSet};
use super::reload::make_responses_request_template;
use super::tools::{self, ToolContext};
use super::{ENV_OPENAI_API_KEY, ENV_OPENAI_MAX_OUTPUT_TOKENS};

use crate::ai::openai_responses::types::input::InputItem;
use crate::ai::openai_responses::types::request::{CreateResponseRequest, ReasoningEffort};
use crate::ai::openai_responses::types::response::OutputItem;
use crate::ai::openai_responses::types::stream::StreamEvent;
use crate::ai::openai_responses::util::extract_output_text;
use crate::ai::openai_responses::{ResponsesClient, ResponsesClientConfig};
use crate::conf::{TwitchEventSubConfig, TwitchModeratorConfig};
use crate::state::AiRuntime;
use crate::{ChannelDatum, SharedChannelData, SharedState};

use anyhow::{anyhow, bail, Result};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tokio::sync::{broadcast, Mutex, RwLock, Semaphore};

const DEFAULT_MEMORY_CAPACITY: usize = 4;
const DEFAULT_REMOVE_CHARS: &str = "\n\r\t";
const DEFAULT_OPENAI_MAX_IN_FLIGHT: usize = 2;
const DEFAULT_OPENAI_STREAM: bool = true;
const DEFAULT_OVERFLOW_SUMMARY_MIN_CHARS: usize = 64;

// ============================================================
// χ-6: conf 値を Responses API の request 値へ resolve するヘルパ。
//
// 優先順位は `env > 新 conf キー > legacy conf キー > None（= API 既定）`。
// 新旧両方指定されても warn は出さない（un-discord 方針に合わせ "warn 無しで fallback"）。
// ============================================================

/// `openai_max_output_tokens` を env / 新 conf / legacy `max_tokens` の順に解決する。
///
/// - env `VAC_OPENAI_MAX_OUTPUT_TOKENS`: 数値パース失敗時は warn ログを出して無視
/// - 新キー `openai_max_output_tokens` (u32) → そのまま
/// - legacy `max_tokens` (u16) → u32 にキャスト
fn resolve_openai_max_output_tokens(persona: &AiPersonaConf) -> Option<u32> {
	if let Ok(raw) = std::env::var(ENV_OPENAI_MAX_OUTPUT_TOKENS) {
		match raw.trim().parse::<u32>() {
			Ok(v) => return Some(v),
			Err(e) => log::warn!(
				"環境変数 {} = {:?} は u32 に解釈できませんでした（{}）。conf のフォールバックを使います。",
				ENV_OPENAI_MAX_OUTPUT_TOKENS,
				raw,
				e
			),
		}
	}
	if let Some(v) = persona.openai_max_output_tokens {
		return Some(v);
	}
	persona.max_tokens.map(u32::from)
}

/// ψ-α: gpt-5 系の tool loop round 間 reasoning pass-through を有効にするかを解決する。
///
/// - conf `openai_reasoning_encrypted_passthrough = true/false` が明示されていればそれを優先
/// - 未指定なら **既定 `true`**（gpt-5 系でのみ実際に有効になり、非 gpt-5 は呼び出し側で no-op）
fn resolve_reasoning_encrypted_passthrough(persona: &AiPersonaConf) -> bool {
	persona.openai_reasoning_encrypted_passthrough.unwrap_or(true)
}

/// ψ-α: `include: ["reasoning.encrypted_content"]` の付与をモデル判定と enabled flag に基づいて適用する。
///
/// - gpt-5 系 + `enabled == true` のときだけ `include` に追加（重複 append しない）
/// - 非 gpt-5 モデル or `enabled == false` では request を変更しない
fn apply_reasoning_passthrough_include(request: &mut CreateResponseRequest, model: Option<&str>, enabled: bool) {
	if !(enabled && model_policy::is_gpt5_family(model)) {
		return;
	}
	let entry = "reasoning.encrypted_content".to_string();
	let list = request.include.get_or_insert_with(Vec::new);
	if !list.iter().any(|s| s == &entry) {
		list.push(entry);
	}
}

/// ψ-α: `response.output` 中の `OutputItem::Reasoning` を次ラウンド input 用の
/// `InputItem::Reasoning` に変換して返す。順序は出力順を維持する。
///
/// 返り値の 2 番目は `encrypted_content` が欠落していた件数。`include` を指定した
/// request に対して API 側が blob を返さないケースを観測したい目的で使う（warn で露出）。
fn collect_reasoning_input_items(output: &[OutputItem]) -> (Vec<InputItem>, usize) {
	let mut items = Vec::new();
	let mut missing = 0usize;
	for item in output {
		if let OutputItem::Reasoning {
			id,
			encrypted_content,
			summary,
			..
		} = item
		{
			if encrypted_content.is_none() {
				missing += 1;
			}
			items.push(InputItem::Reasoning {
				id: id.clone(),
				encrypted_content: encrypted_content.clone(),
				summary: summary.clone(),
			});
		}
	}
	(items, missing)
}

/// `openai_reasoning_effort` の conf enum を Responses API の `ReasoningEffort` に写す。
fn resolve_openai_reasoning_effort(persona: &AiPersonaConf) -> Option<ReasoningEffort> {
	persona.openai_reasoning_effort.map(|e| match e {
		OpenAiReasoningEffortConf::Low => ReasoningEffort::Low,
		OpenAiReasoningEffortConf::Medium => ReasoningEffort::Medium,
		OpenAiReasoningEffortConf::High => ReasoningEffort::High,
	})
}

/// オーバーフロー要約で使う `max_output_tokens` を新キー > legacy の順に解決する。
fn resolve_overflow_summary_max_output_tokens(persona: &AiPersonaConf) -> Option<u32> {
	if let Some(v) = persona.memory_overflow_summary_max_output_tokens {
		return Some(v);
	}
	persona.memory_overflow_summary_max_completion_tokens.map(u32::from)
}

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
	/// χ-5: OpenAI Responses API `/v1/responses` 用の自前 client。
	client: ResponsesClient,
	/// χ-5: Responses API リクエストテンプレート。
	/// `reload::make_responses_request_template()` の結果を atomic swap する。
	request_template: Arc<RwLock<Arc<CreateResponseRequest>>>,
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
		// χ-6: 新 conf キー（`openai_max_output_tokens` / `openai_reasoning_effort` / `openai_store`）を
		// 優先しつつ env / legacy にフォールバックして request_template に詰める。
		let resolved_max_output_tokens = resolve_openai_max_output_tokens(&persona);
		let resolved_reasoning_effort = resolve_openai_reasoning_effort(&persona);
		let resolved_store = Some(persona.openai_store.unwrap_or(false));
		let request_template =
			make_responses_request_template(&persona, resolved_max_output_tokens, resolved_reasoning_effort, resolved_store)?;

		let decision = Arc::new(DecisionSpec::from_persona(&persona)?);
		log::debug!(
			"《AI[{}]》 Decision engine: {:?} (legacy_compat={})",
			persona.id.as_deref().unwrap_or("-"),
			decision,
			persona.decision.is_none(),
		);

		let max_in_flight = persona.openai_max_in_flight.unwrap_or(DEFAULT_OPENAI_MAX_IN_FLIGHT).max(1);

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
		let persona_label = persona_arc.id.clone().unwrap_or_else(|| "-".to_string());

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
				}
				Err(broadcast::error::RecvError::Lagged(n)) => {
					log::warn!("《AI[{}]》 観測が {} 件遅延しました（追いつけなかった分は欠落）。", label, n);
				}
				Err(broadcast::error::RecvError::Closed) => {
					log::info!("《AI[{}]》 event loop を終了します（broadcast が閉じられました）。", label);
					break;
				}
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
			let hb_enabled_now = self.persona.read().await.heartbeat.as_ref().map(|h| h.enabled).unwrap_or(false);
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
				log::trace!("《AI[{}]》 heartbeat: 評価対象のトリガー発話が見つかりません。スキップ。", label);
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
		let memory_overflow_summary_max_output_tokens = resolve_overflow_summary_max_output_tokens(&persona);
		let memory_overflow_summary_max_input_chars = persona.memory_overflow_summary_max_input_chars.unwrap_or(16_000);
		let memory_overflow_summary_min_chars = persona
			.memory_overflow_summary_min_chars
			.unwrap_or(DEFAULT_OVERFLOW_SUMMARY_MIN_CHARS);
		let memory_overflow_summary_cooldown_secs = persona.memory_overflow_summary_cooldown_secs.filter(|&s| s > 0);
		let model_for_runtime = persona.model.clone();
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
		let reasoning_encrypted_passthrough = resolve_reasoning_encrypted_passthrough(&persona);
		let respect_speech_floor = persona.respect_speech_floor.clone();

		let channel_utterance = self.channel_utterance.clone();
		let state = self.state.clone();
		let channel_data = self.channel_data.clone();
		let client = self.client.clone();
		// χ-5: Arc<CreateResponseRequest> として取得し、後続で `(*request_template).clone()` で深い複製を作る。
		let request_template: Arc<CreateResponseRequest> = self.request_template.read().await.clone();
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
			}
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
				}
				Err(context::MemoryWindowError::TriggerNotFound) => {
					log::error!(
      "《AI[{}]》 処理対象の入力は既にありませんでした。必要に応じて設定の state_data_capacity を増やすと解決するかもしれません。",
      persona_label
     );
					return Ok(());
				}
				Err(context::MemoryWindowError::TriggerNotFinal) => {
					log::trace!("《AI[{}]》 未確定の入力なので、処理をスキップします。", persona_label);
					return Ok(());
				}
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
						memory_overflow_summary_max_output_tokens,
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
					memory_overflow_summary_max_output_tokens,
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
			}
			Decision::SkipByScore { score, threshold } => {
				log::trace!(
					"《AI[{}]》 Decision: SkipByScore (score={:.2} < threshold={:.2}) target={:?}",
					persona_label,
					score,
					threshold,
					newest_trimmed
				);
				return Ok(());
			}
			Decision::SkipByMinInterval { remaining_secs } => {
				log::trace!(
					"《AI[{}]》 Decision: SkipByMinInterval（残り {}s）。",
					persona_label,
					remaining_secs
				);
				return Ok(());
			}
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
					}
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

		// χ-5: Responses API 用の request を template から深くコピー。
		let mut request = (*request_template).clone();
		request.input = context::assemble_openai_responses_input(
			model_for_runtime.as_deref(),
			custom_instructions.as_deref(),
			system_instructions_extra.as_deref(),
			memory_summary_combined.as_deref(),
			&openai_few_shot,
			persona_anchor.as_deref(),
			&reversed_sources,
			&observe,
		);

		// ψ-α: gpt-5 系 + passthrough 有効時のみ `include: ["reasoning.encrypted_content"]` を付与。
		// 非 gpt-5 モデルでは no-op（API 側で意味を持たない keyword 指定を避ける）。
		let include_before = request.include.as_ref().map(|v| v.len()).unwrap_or(0);
		apply_reasoning_passthrough_include(&mut request, model_for_runtime.as_deref(), reasoning_encrypted_passthrough);
		if request.include.as_ref().map(|v| v.len()).unwrap_or(0) > include_before {
			log::debug!(
				"《AI[{}]》 ψ-α: include=reasoning.encrypted_content を付与（model={:?}, passthrough={}）。",
				persona_label,
				model_for_runtime,
				reasoning_encrypted_passthrough
			);
		}

		// χ-5: tools は Responses API の `Vec<Tool>` を直接 request.tools に入れる。
		// hosted tools（web_search / file_search / code_interpreter）も passthrough。
		let mut tools_declared_local: std::collections::HashSet<String> = std::collections::HashSet::new();
		let mut has_tools = false;
		if let Some(ref p) = openai_tools_json_path {
			match tokio::fs::read_to_string(p).await {
				Ok(s) => match tools::parse_tools_json(&s) {
					Ok(v) if !v.is_empty() => {
						tools_declared_local = tools::locally_dispatched_tool_names(&v);
						let declared_count = v.len();
						let hosted_count = v.iter().filter(|t| !t.is_locally_dispatched()).count();
						request.tools = Some(v);
						request.tool_choice = match openai_tool_choice.as_deref() {
							None => None,
							Some(tc) => match tools::parse_tool_choice(tc) {
								Ok(x) => Some(x),
								Err(e) => {
									log::warn!("openai_tool_choice を解釈できません: {} — 未指定にします。", e);
									None
								}
							},
						};
						if let Some(ptc) = openai_parallel_tool_calls {
							request.parallel_tool_calls = Some(ptc);
						}
						has_tools = true;
						log::info!(
							"《AI[{}]》 OpenAI tools を読み込みました: {:?} (declared={}, local_dispatchable={}, hosted={})",
							persona_label,
							p,
							declared_count,
							tools_declared_local.len(),
							hosted_count,
						);
					}
					Ok(_) => log::warn!("《AI[{}]》 openai_tools_json_path の tools が空です: {:?}", persona_label, p),
					Err(e) => log::warn!("《AI[{}]》 openai_tools_json をパースできません: {:?} ({})", persona_label, p, e),
				},
				Err(e) => log::warn!(
					"《AI[{}]》 openai_tools_json_path を読めませんでした: {:?} ({})",
					persona_label,
					p,
					e
				),
			}
		}

		let effective_stream = openai_stream;
		let request_max_output_tokens = request.max_output_tokens;

		log::debug!(
			"《AI[{}]》 応答をリクエストします（stream={}, tools={}）。",
			persona_label,
			effective_stream,
			has_tools
		);

		// χ-5: Responses API の stream / non-stream を同じ tool-loop で駆動する。
		// streaming 経路でも OutputItem::FunctionCall が返れば次ラウンドの input[] に積んで再 request、
		// ツール無し応答のラウンドで break。
		let tool_ctx_opt: Option<ToolContext> = if has_tools { Some(self.tool_context().await) } else { None };
		let drive_result = drive_responses_tool_loop(
			&client,
			request,
			effective_stream,
			has_tools,
			&tools_declared_local,
			tool_ctx_opt.as_ref(),
			&state,
			&channel_utterance,
			&persona_label,
		)
		.await;

		let (mut content, stream_datum_id, stream_nonempty_delta_chunks) = match drive_result {
			Ok(out) => (out.content, out.stream_datum_id, out.stream_nonempty_delta_chunks),
			Err(e) => {
				log::error!("《AI[{}]》 OpenAI Responses API 呼び出しに失敗しました: {:?}", persona_label, e);
				eprint_openai_usage_hint_if_needed(&format!("{e:?}"));
				bail!("{e:?}");
			}
		};

		if model_policy::should_retry_on_empty_assistant(model_for_runtime.as_deref()) && content.trim().is_empty() {
			log::warn!(
				"《AI[{}]》 gpt-5 系モデルから空の content が返却されました。再試行します。",
				persona_label
			);
			if let Some(model) = model_for_runtime.as_deref() {
				if let Some(filled) =
					completion::retry_if_gpt5_empty_response(&client, model, &latest_user_content, request_max_output_tokens).await
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
			state
				.read()
				.await
				.finalize_channel_datum_and_dispatch(sid, content, &marker_flag)
				.await;
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
			log::info!(
				"《AI[{}]》 は is_enabled = false のためスキップします。",
				p.id.clone().unwrap_or_else(|| "-".to_string())
			);
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
	client: &ResponsesClient,
	overflow_model: &Option<String>,
	main_model: Option<&str>,
	max_output_tokens: Option<u32>,
	truncated: &str,
	dropped_count: usize,
	last_at: &Arc<Mutex<Option<Instant>>>,
) -> Option<String> {
	let model_ov = match overflow_model.as_deref().or(main_model) {
		Some(m) => m,
		None => {
			log::warn!("AI: `model` が未設定のためオーバーフロー要約をスキップします。");
			return None;
		}
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
	match completion::summarize_overflow_turns(client, model_ov, max_output_tokens, truncated).await {
		Ok(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
		Ok(_) => {
			log::warn!("メモリ窓オーバーフロー要約の結果が空でした。長期メモには載せません。");
			None
		}
		Err(e) => {
			log::warn!("メモリ窓オーバーフロー要約に失敗しました（本応答は続行）: {:?}", e);
			None
		}
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

/// χ-5: Responses API tool-loop driver の戻り値。
struct ToolLoopOutput {
	content: String,
	stream_datum_id: Option<u64>,
	stream_nonempty_delta_chunks: usize,
}

/// χ-5: Responses API の streaming / non-stream を共通 tool-loop で駆動する。
///
/// - streaming: `create_stream` で SSE を読み、`OutputTextDelta` で live 表示を更新、
///   `Completed` / `Incomplete` で `response.output[]` を確定取得。
///   非 streaming: `create` を呼んで `Response` を直接取得。
/// - `OutputItem::FunctionCall` が 1 件でもあれば、それら（と対応 output）を `request.input[]`
///   に積み直して次ラウンドに入る。無ければ assistant text を抽出して break。
///
/// 既定のラウンド上限は 8（旧 Chat Completions 経路と同じ）。
#[allow(clippy::too_many_arguments)]
async fn drive_responses_tool_loop(
	client: &ResponsesClient,
	mut request: CreateResponseRequest,
	streaming: bool,
	has_tools: bool,
	declared_local: &std::collections::HashSet<String>,
	tool_ctx: Option<&ToolContext>,
	state: &SharedState,
	channel_utterance: &str,
	persona_label: &str,
) -> Result<ToolLoopOutput> {
	const MAX_TOOL_ROUNDS: usize = 8;

	let mut content = String::new();
	let mut stream_datum_id: Option<u64> = None;
	let mut stream_nonempty_delta_chunks: usize = 0;

	// ψ-α: caller が `include: ["reasoning.encrypted_content"]` を付けた場合のみ
	// tool loop round 間で Reasoning item を pass-through する。caller は
	// is_gpt5_family(model) && openai_reasoning_encrypted_passthrough チェックを
	// 済ませた上で include を設定するので、ここでは include の内容だけ見る。
	let reasoning_passthrough = request
		.include
		.as_ref()
		.is_some_and(|v| v.iter().any(|s| s == "reasoning.encrypted_content"));

	for round in 0..MAX_TOOL_ROUNDS {
		let response: crate::ai::openai_responses::types::response::Response = if streaming {
			log::debug!("《AI[{}]》 create_stream 開始 (round={})", persona_label, round);
			let stream = client
				.create_stream(request.clone())
				.await
				.map_err(|e| anyhow!("create_stream 失敗: {e}"))?;
			log::debug!("《AI[{}]》 create_stream accepted, SSE 受信待ち (round={})", persona_label, round);
			futures::pin_mut!(stream);

			// FunctionCallArgumentsDelta を item_id 単位で蓄積する（Completed 前に最終 response
			// が届けばそちらを優先して使うが、届かないサーバ実装への保険として持っておく）。
			let mut fc_args_accum: HashMap<String, String> = HashMap::new();
			let mut finalized: Option<crate::ai::openai_responses::types::response::Response> = None;
			let mut failure: Option<String> = None;
			let mut ev_count: usize = 0;

			while let Some(ev) = stream.next().await {
				ev_count += 1;
				match &ev {
					Ok(StreamEvent::Created { .. }) => log::trace!("《AI[{}]》 SSE #{}: Created", persona_label, ev_count),
					Ok(StreamEvent::InProgress { .. }) => log::trace!("《AI[{}]》 SSE #{}: InProgress", persona_label, ev_count),
					Ok(StreamEvent::Completed { .. }) => log::debug!("《AI[{}]》 SSE #{}: Completed", persona_label, ev_count),
					Ok(StreamEvent::Incomplete { .. }) => log::debug!("《AI[{}]》 SSE #{}: Incomplete", persona_label, ev_count),
					Ok(StreamEvent::Failed { .. }) => log::warn!("《AI[{}]》 SSE #{}: Failed", persona_label, ev_count),
					Ok(StreamEvent::Error { error }) => log::warn!(
						"《AI[{}]》 SSE #{}: Error code={:?} msg={:?}",
						persona_label,
						ev_count,
						error.code,
						error.message
					),
					_ => {}
				}
				match ev {
					Ok(StreamEvent::OutputTextDelta { delta, .. }) => {
						if delta.is_empty() {
							continue;
						}
						stream_nonempty_delta_chunks += 1;
						content.push_str(&delta);
						match stream_datum_id {
							None => {
								let cd = ChannelDatum::new(channel_utterance.to_string(), content.clone());
								let sid = cd.get_id();
								state.read().await.push_channel_datum_quiet(cd).await;
								stream_datum_id = Some(sid);
							}
							Some(sid) => {
								state.read().await.update_channel_datum_content_by_id(sid, content.clone()).await;
							}
						}
					}
					Ok(StreamEvent::FunctionCallArgumentsDelta { item_id, delta, .. }) => {
						fc_args_accum.entry(item_id).or_default().push_str(&delta);
					}
					Ok(StreamEvent::FunctionCallArgumentsDone { item_id, arguments, .. }) => {
						fc_args_accum.insert(item_id, arguments);
					}
					Ok(StreamEvent::Completed { response }) | Ok(StreamEvent::Incomplete { response }) => {
						finalized = Some(response);
					}
					Ok(StreamEvent::Failed { response }) => {
						let err = response
							.error
							.as_ref()
							.and_then(|e| e.message.clone())
							.unwrap_or_else(|| "unknown".to_string());
						failure = Some(format!("response.failed: {err}"));
					}
					Ok(StreamEvent::Error { error }) => {
						failure = Some(format!("stream error: code={:?} message={:?}", error.code, error.message));
					}
					Ok(_) => {}
					Err(e) => {
						failure = Some(format!("stream decode error: {e}"));
						break;
					}
				}
				if failure.is_some() {
					break;
				}
			}
			log::debug!(
				"《AI[{}]》 SSE ループ終了 (events={}, finalized={}, failure={:?})",
				persona_label,
				ev_count,
				finalized.is_some(),
				failure
			);
			if let Some(msg) = failure {
				bail!("《AI[{persona_label}]》 Responses SSE で失敗: {msg}");
			}
			let Some(mut resp) = finalized else {
				bail!("《AI[{persona_label}]》 Responses SSE が Completed/Incomplete を受信しないまま終了しました。（events={ev_count}）");
			};

			// FunctionCall item の arguments が stream 終了時に未充填のケース（サーバ実装差異）用フォールバック。
			for item in resp.output.iter_mut() {
				if let OutputItem::FunctionCall { id, arguments, .. } = item {
					if arguments.is_empty() {
						if let Some(acc) = fc_args_accum.remove(id.as_str()) {
							*arguments = acc;
						}
					}
				}
			}
			resp
		} else {
			client.create(request.clone()).await.map_err(|e| anyhow!("create 失敗: {e}"))?
		};

		// function_call 抽出
		let function_calls: Vec<OutputItem> = response
			.output
			.iter()
			.filter(|item| matches!(item, OutputItem::FunctionCall { .. }))
			.cloned()
			.collect();

		if function_calls.is_empty() {
			// final assistant text を確定する。streaming で live に積んできた text と、response の
			// 確定 output_text を比較し、確定側が長ければ（サーバが補った最終テキストを使って）置き換える。
			if !streaming {
				if let Some(t) = extract_output_text(&response) {
					content = t;
				}
			} else if let Some(final_text) = extract_output_text(&response) {
				if final_text.chars().count() > content.chars().count() {
					content = final_text;
				}
			}
			return Ok(ToolLoopOutput {
				content,
				stream_datum_id,
				stream_nonempty_delta_chunks,
			});
		}

		if !has_tools {
			bail!("《AI[{persona_label}]》 tools を宣言していないのに OpenAI が function_call を返しました（想定外）。");
		}
		let ctx = tool_ctx.ok_or_else(|| anyhow!("tool_ctx が未初期化（バグ）"))?;

		// ψ-α: passthrough 有効時、response.output に含まれる Reasoning item を
		// 次ラウンドの input[] に FunctionCall より先に積み直す。OpenAI 公式の推奨形
		// （"all items between the last user message and your function call output are
		//   passed into the next response untouched"）に従う。
		// 壊れた / 欠落 blob は warn + そのまま透過（item 自体は落とさない）。
		if reasoning_passthrough {
			let (reasoning_items, missing_blob) = collect_reasoning_input_items(&response.output);
			let pushed = reasoning_items.len();
			for item in reasoning_items {
				request.input.push(item);
			}
			if missing_blob > 0 {
				log::warn!(
     "《AI[{}]》 ψ-α: include=reasoning.encrypted_content を指定したが Reasoning item {}/{} 件で encrypted_content が欠落（blob 無しで transit）。API 側の応答形状変更を疑う余地あり。",
     persona_label,
     missing_blob,
     pushed
    );
			} else if pushed > 0 {
				log::trace!(
					"《AI[{}]》 ψ-α: Reasoning item {} 件を次ラウンド input に pass-through (round={}).",
					persona_label,
					pushed,
					round
				);
			}
		}

		// 次ラウンド input[] に FunctionCall item をそのまま積み、続けて FunctionCallOutput を積む。
		for fc in &function_calls {
			if let OutputItem::FunctionCall {
				call_id, name, arguments, ..
			} = fc
			{
				request.input.push(InputItem::FunctionCall {
					call_id: call_id.clone(),
					name: name.clone(),
					arguments: arguments.clone(),
				});
			}
		}
		for fc in &function_calls {
			if let Some(view) = fc.as_function_call() {
				let out_item = tools::dispatch_tool_call(view, declared_local, ctx).await;
				request.input.push(out_item);
			}
		}

		log::debug!(
			"《AI[{}]》 tool-loop round={} function_calls={} 次ラウンドへ。",
			persona_label,
			round + 1,
			function_calls.len()
		);
	}

	bail!(
		"《AI[{persona_label}]》 OpenAI ツール呼び出しのラウンド上限（{}）に達しました。",
		MAX_TOOL_ROUNDS
	);
}

fn make_client(conf: &AiPersonaConf) -> Result<ResponsesClient> {
	let api_key = crate::utility::load_from_env_or_conf(ENV_OPENAI_API_KEY, &conf.api_key);
	let Some(api_key) = api_key else {
		bail!(
   "OpenAI の API KEY が設定されていません。環境変数 VAC_OPENAI_API_KEY を設定するか、`[[ai.personas]]` の api_key を設定して下さい。"
  );
	};
	let cfg = ResponsesClientConfig {
		api_key,
		..Default::default()
	};
	ResponsesClient::new(cfg).map_err(|e| anyhow!("ResponsesClient の構築に失敗しました: {e}"))
}

#[cfg(test)]
mod chi6_resolver_tests {
	use super::*;

	fn persona_minimal() -> AiPersonaConf {
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
			openai_max_output_tokens: None,
			max_tokens: None,
			openai_reasoning_effort: None,
			openai_store: None,
			openai_reasoning_encrypted_passthrough: None,
			temperature: None,
			top_p: None,
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
			memory_overflow_summary_max_output_tokens: None,
			memory_overflow_summary_max_completion_tokens: None,
			memory_overflow_summary_max_input_chars: None,
			memory_overflow_summary_min_chars: None,
			memory_overflow_summary_cooldown_secs: None,
			memory_summary: None,
			memory_summary_path: None,
			openai_few_shot: vec![],
			persona_anchor: None,
			force_activate_regex_pattern: None,
			ignore_regex_pattern: None,
			min_interval_in_secs: None,
			remove_chars: None,
			assistant_max_chars: None,
			assistant_strip_substrings: vec![],
			openai_stream: None,
			openai_tools_json_path: None,
			openai_tool_choice: None,
			openai_parallel_tool_calls: None,
			openai_max_in_flight: None,
			fine_tuning: None,
			respect_speech_floor: None,
		}
	}

	/// env を絶対にいじりたくないテスト（他の #[test] と環境変数を共有するのを避ける）用に
	/// 一時的に `unset` して実行する RAII ガード。
	struct EnvUnset {
		key: &'static str,
		prev: Option<String>,
	}

	impl EnvUnset {
		fn new(key: &'static str) -> Self {
			let prev = std::env::var(key).ok();
			std::env::remove_var(key);
			Self { key, prev }
		}
	}

	impl Drop for EnvUnset {
		fn drop(&mut self) {
			match self.prev.take() {
				Some(v) => std::env::set_var(self.key, v),
				None => std::env::remove_var(self.key),
			}
		}
	}

	/// env var を触るテストは parallel 実行で衝突するため、1 つの `#[test]` に寄せて
	/// 内部でケースを順番に検証する。`EnvUnset` で test 終了時に元の値へ戻す。
	#[test]
	fn max_output_tokens_precedence_env_new_legacy() {
		let _guard = EnvUnset::new(ENV_OPENAI_MAX_OUTPUT_TOKENS);

		// (a) env / 新 / legacy すべて未指定 → None
		{
			let p = persona_minimal();
			assert_eq!(resolve_openai_max_output_tokens(&p), None, "all unset -> None");
		}

		// (b) legacy のみ指定 → u32 昇格
		{
			let mut p = persona_minimal();
			p.max_tokens = Some(256);
			assert_eq!(resolve_openai_max_output_tokens(&p), Some(256), "legacy only");
		}

		// (c) 新キー優先で legacy を上書き
		{
			let mut p = persona_minimal();
			p.openai_max_output_tokens = Some(1024);
			p.max_tokens = Some(256);
			assert_eq!(resolve_openai_max_output_tokens(&p), Some(1024), "new over legacy");
		}

		// (d) env が妥当な u32 → 新 / legacy を上書き
		{
			std::env::set_var(ENV_OPENAI_MAX_OUTPUT_TOKENS, "2048");
			let mut p = persona_minimal();
			p.openai_max_output_tokens = Some(1024);
			p.max_tokens = Some(256);
			assert_eq!(resolve_openai_max_output_tokens(&p), Some(2048), "env wins");
		}

		// (e) env が不正値 → 新 / legacy にフォールバック
		{
			std::env::set_var(ENV_OPENAI_MAX_OUTPUT_TOKENS, "not-a-number");
			let mut p = persona_minimal();
			p.openai_max_output_tokens = Some(555);
			assert_eq!(resolve_openai_max_output_tokens(&p), Some(555), "invalid env -> fallback to new");
			p.openai_max_output_tokens = None;
			p.max_tokens = Some(111);
			assert_eq!(resolve_openai_max_output_tokens(&p), Some(111), "invalid env -> fallback to legacy");
		}

		// (f) env を空文字に → 空文字は u32::from_str で err → fallback
		{
			std::env::set_var(ENV_OPENAI_MAX_OUTPUT_TOKENS, "");
			let mut p = persona_minimal();
			p.openai_max_output_tokens = Some(999);
			assert_eq!(resolve_openai_max_output_tokens(&p), Some(999), "empty env -> fallback");
		}
	}

	#[test]
	fn reasoning_effort_maps_each_variant() {
		let mut p = persona_minimal();
		p.openai_reasoning_effort = Some(OpenAiReasoningEffortConf::Low);
		assert_eq!(resolve_openai_reasoning_effort(&p), Some(ReasoningEffort::Low));
		p.openai_reasoning_effort = Some(OpenAiReasoningEffortConf::Medium);
		assert_eq!(resolve_openai_reasoning_effort(&p), Some(ReasoningEffort::Medium));
		p.openai_reasoning_effort = Some(OpenAiReasoningEffortConf::High);
		assert_eq!(resolve_openai_reasoning_effort(&p), Some(ReasoningEffort::High));
		p.openai_reasoning_effort = None;
		assert_eq!(resolve_openai_reasoning_effort(&p), None);
	}

	#[test]
	fn overflow_summary_max_prefers_new_key_over_legacy() {
		let mut p = persona_minimal();
		p.memory_overflow_summary_max_output_tokens = Some(800);
		p.memory_overflow_summary_max_completion_tokens = Some(200);
		assert_eq!(resolve_overflow_summary_max_output_tokens(&p), Some(800));
	}

	#[test]
	fn overflow_summary_max_falls_back_to_legacy() {
		let mut p = persona_minimal();
		p.memory_overflow_summary_max_completion_tokens = Some(200);
		assert_eq!(resolve_overflow_summary_max_output_tokens(&p), Some(200));
	}

	#[test]
	fn reasoning_effort_conf_serde_lowercase() {
		// toml で書く小文字表記 (`"low"` / `"medium"` / `"high"`) を受理できること。
		let low: OpenAiReasoningEffortConf = serde_json::from_str("\"low\"").unwrap();
		let medium: OpenAiReasoningEffortConf = serde_json::from_str("\"medium\"").unwrap();
		let high: OpenAiReasoningEffortConf = serde_json::from_str("\"high\"").unwrap();
		assert_eq!(low, OpenAiReasoningEffortConf::Low);
		assert_eq!(medium, OpenAiReasoningEffortConf::Medium);
		assert_eq!(high, OpenAiReasoningEffortConf::High);

		// 大文字表記は受理しない。
		let err = serde_json::from_str::<OpenAiReasoningEffortConf>("\"HIGH\"");
		assert!(err.is_err());
	}

	// ============================================================
	// ψ-α: encrypted reasoning passthrough
	// ============================================================

	#[test]
	fn reasoning_encrypted_passthrough_defaults_to_true() {
		let p = persona_minimal();
		assert!(
			resolve_reasoning_encrypted_passthrough(&p),
			"未指定時は既定 true（gpt-5 系で実際に有効、非 gpt-5 は caller 側で no-op）"
		);
	}

	#[test]
	fn reasoning_encrypted_passthrough_respects_opt_out() {
		let mut p = persona_minimal();
		p.openai_reasoning_encrypted_passthrough = Some(false);
		assert!(!resolve_reasoning_encrypted_passthrough(&p), "明示的 false は尊重");
	}

	#[test]
	fn apply_reasoning_passthrough_include_noop_when_disabled() {
		let mut req = CreateResponseRequest::default();
		apply_reasoning_passthrough_include(&mut req, Some("gpt-5-mini"), false);
		assert!(req.include.is_none(), "disabled なら include を触らない");
	}

	#[test]
	fn apply_reasoning_passthrough_include_noop_for_non_gpt5() {
		let mut req = CreateResponseRequest::default();
		apply_reasoning_passthrough_include(&mut req, Some("gpt-4o-mini"), true);
		assert!(req.include.is_none(), "非 gpt-5 モデルでは no-op");
	}

	#[test]
	fn apply_reasoning_passthrough_include_sets_for_gpt5() {
		let mut req = CreateResponseRequest::default();
		apply_reasoning_passthrough_include(&mut req, Some("gpt-5-mini"), true);
		assert_eq!(
			req.include.as_ref().map(|v| v.as_slice()),
			Some(&["reasoning.encrypted_content".to_string()][..])
		);
	}

	#[test]
	fn apply_reasoning_passthrough_include_dedupes() {
		let mut req = CreateResponseRequest {
			include: Some(vec!["reasoning.encrypted_content".to_string()]),
			..Default::default()
		};
		apply_reasoning_passthrough_include(&mut req, Some("gpt-5-mini"), true);
		assert_eq!(req.include.as_ref().unwrap().len(), 1, "既に入っていれば重複 append しない");
	}

	#[test]
	fn apply_reasoning_passthrough_include_handles_unknown_model() {
		let mut req = CreateResponseRequest::default();
		apply_reasoning_passthrough_include(&mut req, None, true);
		assert!(req.include.is_none(), "model 未指定でも no-op（safe default）");
	}

	#[test]
	fn collect_reasoning_input_items_preserves_order_and_flags_missing_blobs() {
		use crate::ai::openai_responses::types::{MessageContent, OutputItem};
		let output = vec![
			OutputItem::Reasoning {
				id: "rs_a".into(),
				status: None,
				summary: Some(serde_json::json!([])),
				encrypted_content: Some("blob_a".into()),
			},
			// 間に Message が挟まっていても Reasoning だけを抽出する。
			OutputItem::Message {
				id: "msg_a".into(),
				status: None,
				role: "assistant".into(),
				content: vec![MessageContent::OutputText {
					text: "hi".into(),
					annotations: None,
				}],
			},
			OutputItem::Reasoning {
				id: "rs_b".into(),
				status: None,
				summary: None,
				encrypted_content: None, // blob 欠落
			},
		];
		let (items, missing) = collect_reasoning_input_items(&output);
		assert_eq!(items.len(), 2);
		assert_eq!(missing, 1, "blob 欠落は 1 件");
		match &items[0] {
			InputItem::Reasoning { id, encrypted_content, .. } => {
				assert_eq!(id, "rs_a");
				assert_eq!(encrypted_content.as_deref(), Some("blob_a"));
			}
			_ => panic!("[0] must be Reasoning"),
		}
		match &items[1] {
			InputItem::Reasoning { id, encrypted_content, .. } => {
				assert_eq!(id, "rs_b");
				assert!(encrypted_content.is_none(), "blob は None で保持");
			}
			_ => panic!("[1] must be Reasoning"),
		}
	}

	#[test]
	fn collect_reasoning_input_items_returns_empty_when_no_reasoning() {
		use crate::ai::openai_responses::types::{MessageContent, OutputItem};
		let output = vec![OutputItem::Message {
			id: "msg_only".into(),
			status: None,
			role: "assistant".into(),
			content: vec![MessageContent::OutputText {
				text: "hi".into(),
				annotations: None,
			}],
		}];
		let (items, missing) = collect_reasoning_input_items(&output);
		assert!(items.is_empty());
		assert_eq!(missing, 0);
	}
}
