//! AI サービス設定型。旧 `feature = "openai-chat"` プロセッサーを常駐 AI サービスへ昇格させる過程で、
//! `[[ai.personas]]` を正式な設定の入口とした（Phase I）。複数ペルソナを配列で持てる。
//!
//! # 設計メモ
//!
//! - **`observe`**: 「観測（＝発火判定）」と「会話文脈への取り込み」を分離せず、素朴に一つのブロックにまとめた。
//!   - `triggers`: ここに列挙したチャンネルからの発火時のみ応答を評価する（従来の `channel_from` の拡張）。
//!   - `include_all` / `include_additional` / `exclude`: 文脈窓（memory window）に載せる範囲。
//! - **`channel_utterance`**: ペルソナが発話（出力）するチャンネル（従来の `channel_to`）。
//! - 旧 `feature = "openai-chat"` の `ProcessorConf` 配下にあったフィールドはほぼそのまま `AiPersonaConf` に移設した。
//!   Phase II 以降に Decision Engine を足す際は `decision` 等の新ブロックを別途追加していく。

use crate::utility::bool_true;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AiConf {
 #[serde(default)]
 pub personas: Vec<AiPersonaConf>,
}

/// Phase II で導入した Decision Engine の設定。
///
/// 基本の数式は `final_score = base_score + Σ modulators + jitter` で、`final_score >= threshold` のときだけ応答する。
/// 旧 `force_activate_regex_pattern` / `ignore_regex_pattern` は **互換レイヤとして**内部的に `modulators` へ合成される
/// （旧フィールドを残しておけば従来通りの挙動になる）。明示的に `[ai.personas.decision]` を書いた場合はそちらが優先。
///
/// `min_interval_in_secs` は **スコアとは独立した hard gate** として残す。連打を抑えたいだけの要件で
/// スコア設計を歪めないための単純な絞り弁で、`force_activate`（スコア >= threshold）後でも効く。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DecisionConf {
 /// 合計スコアがこの値以上のとき応答。未指定時は 50.0。
 #[serde(default = "DecisionConf::default_threshold")]
 pub threshold: f64,
 /// 各観測に常に加算される基礎スコア。未指定時は 0.0。
 #[serde(default)]
 pub base_score: f64,
 /// 合計スコアに加算するランダム揺らぎの絶対値。未指定時は 0.0。`±jitter` の一様分布を加える。
 #[serde(default)]
 pub jitter: f64,
 /// スコア算出器の配列。上から順に評価し、各 modulator の出す値を合算する。
 #[serde(default)]
 pub modulators: Vec<DecisionModulator>,
 /// 応答間隔の下限（秒）。**スコアとは独立した hard gate**。未指定または 0 で無効。
 pub min_interval_in_secs: Option<u64>,
}

impl Default for DecisionConf {
 fn default() -> Self {
  Self {
   threshold: Self::default_threshold(),
   base_score: 0.0,
   jitter: 0.0,
   modulators: Vec::new(),
   min_interval_in_secs: None,
  }
 }
}

impl DecisionConf {
 fn default_threshold() -> f64 {
  50.0
 }
}

/// 個別のスコア算出器。
///
/// 将来 Phase IV（actions）用に `TimeOfDay`（時間帯重み付け）や `Streak`（連続発話ボーナス）などの variant を
/// 追加していく想定。既存値の名前空間を割らないよう `#[serde(tag = "kind")]` で内部タグ化している。
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionModulator {
 /// 対象文字列が正規表現にマッチしたときだけ `score` を加算する（マイナスも可）。
 RegexMatch { pattern: String, score: f64 },
 /// 対象文字列が正規表現にマッチしたとき **非常に大きな負の値** を加算し、事実上ブロックする。
 RegexIgnore { pattern: String },
 /// 観測チャンネルが `channels` のいずれかに一致したら `score` を加算する。
 ChannelMatch {
  #[serde(default)]
  channels: Vec<String>,
  score: f64,
 },
 /// 常に `score` を加算する（底上げ／常時ボーナス）。`base_score` と意味は同じだが、
 /// YAML 的に modulator リスト内で完結させたいときに使える。
 Always { score: f64 },
 /// 前回の発話から `threshold_secs` 秒以上沈黙しているとき、超過秒 × `score_per_sec` を加算する
 /// （`max_score` で上限クリップ）。Heartbeat と組み合わせることで「黙っているほど話したくなる」を表現する。
 SilenceSince {
  threshold_secs: u64,
  score_per_sec: f64,
  #[serde(default = "DecisionModulator::default_silence_max_score")]
  max_score: f64,
 },
}

impl DecisionModulator {
 fn default_silence_max_score() -> f64 {
  100.0
 }
}

/// Phase III で追加した heartbeat 設定（自発発話の外部トリガー）。
///
/// 有効化すると、各ペルソナに 1 本の `tokio::task` が追加され、`interval_secs` ごとに **直近のトリガー発話を再評価** する。
/// 再評価は Decision Engine を通して通常の `react()` と同じ経路で行なうため、`modulators` に
/// `SilenceSince` などを足しておかないと単純な連打になる点に注意。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HeartbeatConf {
 /// 自発発話の評価を有効化する（既定: false）。
 #[serde(default)]
 pub enabled: bool,
 /// 評価の周期秒。未指定時は 60 秒。短くしすぎると料金・レート制限に跳ねるので注意。
 #[serde(default = "HeartbeatConf::default_interval_secs")]
 pub interval_secs: u64,
}

impl Default for HeartbeatConf {
 fn default() -> Self {
  Self {
   enabled: false,
   interval_secs: Self::default_interval_secs(),
  }
 }
}

impl HeartbeatConf {
 fn default_interval_secs() -> u64 {
  60
 }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ObservePolicyConf {
 /// 発火判定対象のチャンネル群（ここに含まれるチャンネルからの Datum だけが反応の起点になる）。
 #[serde(default)]
 pub triggers: Vec<String>,
 /// true のとき、`is_final` な全チャンネルの発話を memory window に載せる（軽量観測）。
 #[serde(default)]
 pub include_all: bool,
 /// `triggers` / `channel_utterance` に加えて追加で文脈として取り込むチャンネル。`include_all = true` のときは無視。
 #[serde(default)]
 pub include_additional: Vec<String>,
 /// `include_all = true` のときの除外リスト、あるいは `include_additional` の打ち消しに使う。
 #[serde(default)]
 pub exclude: Vec<String>,
}

/// 旧 `OpenAiChatFinetuning` と同等（Phase I で `ai` 配下へ移設）。
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OpenAiChatFinetuning {
 Path(String),
 Detail {
  train_path: String,
  validation_path: Option<String>,
  model: Option<String>,
  suffix: Option<String>,
 },
}

impl OpenAiChatFinetuning {
 pub fn to_tuple_for_input(&self) -> (String, Option<String>, Option<String>, Option<String>) {
  match self {
   OpenAiChatFinetuning::Path(train_path) => (train_path.clone(), None, None, None),
   OpenAiChatFinetuning::Detail {
    train_path,
    validation_path,
    model,
    suffix,
    ..
   } => (train_path.clone(), validation_path.clone(), model.clone(), suffix.clone()),
  }
 }

 pub fn train_path(&self) -> String {
  match self {
   OpenAiChatFinetuning::Path(train_path) => train_path.clone(),
   OpenAiChatFinetuning::Detail { train_path, .. } => train_path.clone(),
  }
 }
}

/// few-shot 例（`role`: `user` / `assistant` / `system`）。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAiFewShotTurn {
 pub role: String,
 pub content: String,
}

/// `[[ai.personas]].openai_reasoning_effort` に書く値。gpt-5 系モデル専用。
///
/// Responses API の `reasoning.effort` (`ReasoningEffort` enum) に 1:1 でマップされる。
/// conf 層では async_openai / openai_responses の型に依存したくないので、conf 側の enum として
/// 独立に持ち、変換は `src/ai/service.rs` で行う（χ-6）。
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiReasoningEffortConf {
 Low,
 Medium,
 High,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiPersonaConf {
 /// 任意の識別子。ログ・CLI の `--persona-id` 指定に使う。未指定でもサービスは起動するが、Fine-tune 等の CLI 指定が効かない。
 pub id: Option<String>,
 #[serde(default = "bool_true")]
 pub is_enabled: bool,

 /// このペルソナが発話するチャンネル（旧 `channel_to`）。
 pub channel_utterance: Option<String>,

 /// Phase IV: `vac_emit_effect` ツールが既定で出力する VAC チャンネル名。ツール引数で `channel` を明示しない場合に使う。
 /// 未指定時は `"effect"`。ブラウザソースは `effect` 等のチャンネルに流れる `ChannelDatum` を WebSocket 経由で受ける想定。
 pub channel_effect: Option<String>,

 #[serde(default)]
 pub observe: ObservePolicyConf,

 /// Phase II で追加した Decision Engine の明示設定。指定があれば legacy の
 /// `force_activate_regex_pattern` / `ignore_regex_pattern` / `min_interval_in_secs` よりも優先される。
 pub decision: Option<DecisionConf>,

 /// Phase III で追加した heartbeat（自発発話の外部トリガー）設定。未指定なら無効。
 pub heartbeat: Option<HeartbeatConf>,

 // --- OpenAI API ---
 pub api_key: Option<String>,
 pub model: Option<String>,
 pub custom_instructions: Option<String>,
 pub system_instructions_extra: Option<String>,
 /// χ-6 で追加した Responses API 正式キー。未指定時は env `VAC_OPENAI_MAX_OUTPUT_TOKENS`、
 /// それも無ければ legacy `max_tokens`（Chat Completions 由来の u16）が fallback に使われる。
 /// 両方指定時は **新キーが優先**（`max_tokens` は warn 無しで無視）。
 pub openai_max_output_tokens: Option<u32>,
 /// χ-6 で追加した Chat Completions 互換 legacy フィールド。`openai_max_output_tokens` が
 /// 未指定のときだけ fallback として u32 へ昇格させて扱う。
 pub max_tokens: Option<u16>,
 /// χ-6 で追加。gpt-5 系の `reasoning.effort` を制御する。低 / 中 / 高で切り替え可能。
 /// gpt-5 系以外で指定しても Responses 層で無視される（warn ログ）。
 pub openai_reasoning_effort: Option<OpenAiReasoningEffortConf>,
 /// χ-6 で追加。Responses API の `store` フラグ。VAC は client 側で全 input を再構築する
 /// ので、**未指定時は `false`** を送る（サーバ側 conversation state に依存しない運用）。
 /// `previous_response_id` を活用する将来 phase で true 化する余地を残す。
 pub openai_store: Option<bool>,
 pub temperature: Option<f32>,
 pub top_p: Option<f32>,
 pub n: Option<u8>,
 pub presence_penalty: Option<f32>,
 pub frequency_penalty: Option<f32>,
 pub user: Option<String>,

 // --- Memory ---
 pub memory_capacity: Option<usize>,
 pub memory_max_chars: Option<usize>,
 pub memory_budget_approx_tokens: Option<usize>,
 pub memory_budget_chars_per_approx_token: Option<u8>,
 pub memory_overflow_summary_enabled: Option<bool>,
 pub memory_overflow_summary_model: Option<String>,
 /// χ-6 で追加した Responses API 正式キー。オーバーフロー要約呼び出しの
 /// `max_output_tokens` に使う。未指定時は legacy `memory_overflow_summary_max_completion_tokens`
 /// が u32 に昇格して fallback する。
 pub memory_overflow_summary_max_output_tokens: Option<u32>,
 /// Chat Completions 時代の legacy フィールド（χ-6 から deprecated 扱い）。
 /// `memory_overflow_summary_max_output_tokens` が未指定のときだけ fallback に使う。
 pub memory_overflow_summary_max_completion_tokens: Option<u16>,
 pub memory_overflow_summary_max_input_chars: Option<usize>,
 pub memory_overflow_summary_min_chars: Option<usize>,
 pub memory_overflow_summary_cooldown_secs: Option<u64>,
 pub memory_summary: Option<String>,
 pub memory_summary_path: Option<PathBuf>,

 // --- Few-shot / Anchor ---
 #[serde(default)]
 pub openai_few_shot: Vec<OpenAiFewShotTurn>,
 pub persona_anchor: Option<String>,

 // --- Trigger / Behavior (Phase I 互換。Phase II で decision engine に段階移行する) ---
 pub force_activate_regex_pattern: Option<String>,
 pub ignore_regex_pattern: Option<String>,
 pub min_interval_in_secs: Option<u64>,
 pub remove_chars: Option<String>,
 pub assistant_max_chars: Option<usize>,
 #[serde(default)]
 pub assistant_strip_substrings: Vec<String>,
 pub openai_stream: Option<bool>,
 pub openai_tools_json_path: Option<PathBuf>,
 pub openai_tool_choice: Option<String>,
 pub openai_parallel_tool_calls: Option<bool>,
 pub openai_max_in_flight: Option<usize>,
 pub fine_tuning: Option<OpenAiChatFinetuning>,
 pub respect_speech_floor: Option<String>,
}
