//! Decision Engine（ユーティリティ・スコアリング）— Phase II で導入。
//!
//! # 設計
//!
//! 旧 `OpenAiChat` の「`force_activate_regex` / `ignore_regex` / `min_interval_in_secs` の直列チェック」を、
//! **合計スコア（`base_score + Σ modulators + jitter`）が `threshold` を超えたら応答** という統一的な形に差し替える。
//! これにより:
//!
//! - Phase III の Heartbeat（外部観測がなくても定期的に評価し、スコアが通ったら自発発話）で同じ数式を使える。
//! - Phase IV の Actions（発話以外の副作用）を別 variant として生やしやすい。
//! - 「聞き流し」「強く反応」「中間のふんわり判断」を連続的に表現できる（ON/OFF のツリーでなくなる）。
//!
//! # 互換
//!
//! `AiPersonaConf.decision` が `None` のときは [`DecisionSpec::from_legacy`] が旧フィールドから
//! equivalent な `DecisionConf` を合成するので、既存の `force_activate_regex_pattern` /
//! `ignore_regex_pattern` / `min_interval_in_secs` を書いている設定はそのまま動く。
//!
//! `min_interval_in_secs` は **スコアとは独立した hard gate** として維持する（過剰な短周期抑制を
//! スコア側に無理に埋め込まないための単純な絞り弁）。

use super::config::{AiPersonaConf, DecisionConf, DecisionModulator};

use anyhow::Result;
use regex::Regex;
use std::time::SystemTime;

/// 明示的なブロック用の巨大負スコア。`threshold` を数百程度に取っていても確実に沈む値にしておく。
/// これ自体は設定に出さず、`RegexIgnore` の内部動作として使う。
const BLOCKING_SCORE: f64 = -1.0e12;

/// 互換用の「強制発火」加点。旧 `force_activate_regex_pattern` を合成するときに使う（threshold を
/// 余裕で超える大きさ）。ユーザーが明示的に `modulators` を書けば任意の値にできる。
const LEGACY_FORCE_ACTIVATE_SCORE: f64 = 1.0e9;

/// 評価時に観測を渡すための入力。
///
/// `reversed_sources` ではなく「この観測の直近トリガー発話の本文」単独で十分なので、
/// シンプルに `&str` で渡す。将来 `channel` や添付情報を使うモジュレータを増やすことを想定して struct 化している。
#[derive(Debug, Clone)]
pub struct DecisionInput<'a> {
 pub channel: &'a str,
 pub latest_trigger_content: &'a str,
}

/// 応答するか否か、しない場合の簡潔な理由。
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
 Activate { score: f64, threshold: f64 },
 SkipByScore { score: f64, threshold: f64 },
 SkipByMinInterval { remaining_secs: u64 },
}

#[derive(Debug)]
enum CompiledModulator {
 RegexMatch { re: Regex, score: f64 },
 RegexIgnore { re: Regex },
 ChannelMatch { channels: Vec<String>, score: f64 },
 Always { score: f64 },
 SilenceSince {
  threshold_secs: u64,
  score_per_sec: f64,
  max_score: f64,
 },
}

impl CompiledModulator {
 fn contribution(&self, input: &DecisionInput<'_>, last_activated: SystemTime) -> f64 {
  match self {
   Self::RegexMatch { re, score } => {
    if re.is_match(input.latest_trigger_content) {
     *score
    } else {
     0.0
    }
   },
   Self::RegexIgnore { re } => {
    if re.is_match(input.latest_trigger_content) {
     BLOCKING_SCORE
    } else {
     0.0
    }
   },
   Self::ChannelMatch { channels, score } => {
    if channels.iter().any(|c| c == input.channel) {
     *score
    } else {
     0.0
    }
   },
   Self::Always { score } => *score,
   Self::SilenceSince {
    threshold_secs,
    score_per_sec,
    max_score,
   } => {
    let elapsed = last_activated.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    if elapsed <= *threshold_secs {
     0.0
    } else {
     let over = (elapsed - threshold_secs) as f64;
     (over * score_per_sec).min(*max_score).max(0.0)
    }
   },
  }
 }
}

/// `DecisionConf` をコンパイルし、毎観測時に高速に評価できるようにしたもの。
pub struct DecisionSpec {
 threshold: f64,
 base_score: f64,
 jitter: f64,
 modulators: Vec<CompiledModulator>,
 min_interval_in_secs: Option<u64>,
}

impl std::fmt::Debug for DecisionSpec {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  f.debug_struct("DecisionSpec")
   .field("threshold", &self.threshold)
   .field("base_score", &self.base_score)
   .field("jitter", &self.jitter)
   .field("modulators_count", &self.modulators.len())
   .field("min_interval_in_secs", &self.min_interval_in_secs)
   .finish()
 }
}

impl DecisionSpec {
 /// 明示的な `DecisionConf` からコンパイル。
 pub fn from_conf(conf: &DecisionConf) -> Result<Self> {
  let modulators = conf
   .modulators
   .iter()
   .map(CompiledModulator::try_from_conf)
   .collect::<Result<Vec<_>>>()?;
  Ok(Self {
   threshold: conf.threshold,
   base_score: conf.base_score,
   jitter: conf.jitter,
   modulators,
   min_interval_in_secs: conf.min_interval_in_secs.filter(|&s| s > 0),
  })
 }

 /// `AiPersonaConf` から互換的に合成する。`persona.decision` があればそれを優先し、無ければ legacy フィールドから組む。
 pub fn from_persona(persona: &AiPersonaConf) -> Result<Self> {
  if let Some(ref decision) = persona.decision {
   return Self::from_conf(decision);
  }
  Self::from_legacy(persona)
 }

 /// 旧フィールド（`force_activate_regex_pattern` / `ignore_regex_pattern` / `min_interval_in_secs`）から
 /// 等価な Decision Spec を合成する。threshold=0.0・base_score=50.0 にしておけば、
 /// 「特別な modulator が無い観測は常に応答する（=旧挙動）」という不変を保てる。
 pub fn from_legacy(persona: &AiPersonaConf) -> Result<Self> {
  let mut modulators = Vec::new();

  if let Some(ref pat) = persona.ignore_regex_pattern {
   modulators.push(CompiledModulator::RegexIgnore { re: Regex::new(pat)? });
  }
  if let Some(ref pat) = persona.force_activate_regex_pattern {
   modulators.push(CompiledModulator::RegexMatch {
    re: Regex::new(pat)?,
    score: LEGACY_FORCE_ACTIVATE_SCORE,
   });
  }

  Ok(Self {
   threshold: 0.0,
   base_score: 50.0,
   jitter: 0.0,
   modulators,
   min_interval_in_secs: persona.min_interval_in_secs.filter(|&s| s > 0),
  })
 }

 /// `last_activated` は現在の `SystemTime` と比較して min_interval を計算する用。
 /// `jitter_supplier` は `-jitter..+jitter` の値を返すクロージャ（テストでは固定値、本番では乱数）。
 pub fn evaluate(
  &self,
  input: &DecisionInput<'_>,
  last_activated: SystemTime,
  jitter_supplier: impl FnOnce(f64) -> f64,
 ) -> Decision {
  if let Some(min_secs) = self.min_interval_in_secs {
   if let Ok(elapsed) = last_activated.elapsed() {
    let elapsed_secs = elapsed.as_secs();
    if elapsed_secs < min_secs {
     return Decision::SkipByMinInterval {
      remaining_secs: min_secs - elapsed_secs,
     };
    }
   }
  }

  let mut score = self.base_score;
  for m in &self.modulators {
   score += m.contribution(input, last_activated);
  }
  score += jitter_supplier(self.jitter);

  if score >= self.threshold {
   Decision::Activate {
    score,
    threshold: self.threshold,
   }
  } else {
   Decision::SkipByScore {
    score,
    threshold: self.threshold,
   }
  }
 }

 /// スコア計算で触る `min_interval_in_secs` の値（hard gate 側）を取得。`AiService` 側で
 /// `last_activated` を更新するかどうかの判定に使う。
 pub fn min_interval_in_secs(&self) -> Option<u64> {
  self.min_interval_in_secs
 }
}

impl CompiledModulator {
 fn try_from_conf(m: &DecisionModulator) -> Result<Self> {
  Ok(match m {
   DecisionModulator::RegexMatch { pattern, score } => Self::RegexMatch {
    re: Regex::new(pattern)?,
    score: *score,
   },
   DecisionModulator::RegexIgnore { pattern } => Self::RegexIgnore { re: Regex::new(pattern)? },
   DecisionModulator::ChannelMatch { channels, score } => Self::ChannelMatch {
    channels: channels.clone(),
    score: *score,
   },
   DecisionModulator::Always { score } => Self::Always { score: *score },
   DecisionModulator::SilenceSince {
    threshold_secs,
    score_per_sec,
    max_score,
   } => Self::SilenceSince {
    threshold_secs: *threshold_secs,
    score_per_sec: *score_per_sec,
    max_score: *max_score,
   },
  })
 }
}

/// 既定の jitter サプライヤ。`rand` による一様乱数を返す。
pub fn uniform_jitter(jitter: f64) -> f64 {
 if jitter.abs() < f64::EPSILON {
  0.0
 } else {
  (rand::random::<f64>() * 2.0 - 1.0) * jitter
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 use std::time::Duration;

 fn no_jitter(_: f64) -> f64 {
  0.0
 }

 fn persona_with_legacy(force: Option<&str>, ignore: Option<&str>, min_interval: Option<u64>) -> AiPersonaConf {
  let mut p = AiPersonaConf {
   id: Some("t".into()),
   is_enabled: true,
   channel_utterance: Some("ai".into()),
   channel_effect: None,
   observe: Default::default(),
   decision: None,
   heartbeat: None,
   api_key: None,
   model: None,
   custom_instructions: None,
   system_instructions_extra: None,
   max_tokens: None,
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
   memory_overflow_summary_max_completion_tokens: None,
   memory_overflow_summary_max_input_chars: None,
   memory_overflow_summary_min_chars: None,
   memory_overflow_summary_cooldown_secs: None,
   memory_summary: None,
   memory_summary_path: None,
   openai_few_shot: vec![],
   persona_anchor: None,
   force_activate_regex_pattern: force.map(str::to_string),
   ignore_regex_pattern: ignore.map(str::to_string),
   min_interval_in_secs: min_interval,
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
  };
  p.observe.triggers = vec!["user".into()];
  p
 }

 #[test]
 fn legacy_default_activates_without_modulators() {
  let p = persona_with_legacy(None, None, None);
  let spec = DecisionSpec::from_persona(&p).unwrap();
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "hello",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(d, Decision::Activate { .. }), "got {:?}", d);
 }

 #[test]
 fn legacy_ignore_blocks_activation() {
  let p = persona_with_legacy(None, Some("^NG$"), None);
  let spec = DecisionSpec::from_persona(&p).unwrap();
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "NG",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(d, Decision::SkipByScore { .. }), "got {:?}", d);
 }

 #[test]
 fn legacy_ignore_wins_over_force_when_both_match() {
  // 旧 OpenAiChat の挙動では ignore_regex_pattern を先に評価して早期 return していたため、
  // ignore が勝つのが仕様。新エンジンでも BLOCKING_SCORE は LEGACY_FORCE_ACTIVATE_SCORE より
  // 十分大きい（絶対値で）ため同じ結果になる。
  let p = persona_with_legacy(Some("hello"), Some("hello"), None);
  let spec = DecisionSpec::from_persona(&p).unwrap();
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "hello",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(d, Decision::SkipByScore { .. }), "got {:?}", d);
 }

 #[test]
 fn legacy_force_regex_activates_without_ignore_conflict() {
  // force がマッチするだけなら当然 Activate。
  let p = persona_with_legacy(Some("hello"), None, None);
  let spec = DecisionSpec::from_persona(&p).unwrap();
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "hello there",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(d, Decision::Activate { .. }), "got {:?}", d);
 }

 #[test]
 fn min_interval_gates_even_when_score_passes() {
  let p = persona_with_legacy(None, None, Some(5));
  let spec = DecisionSpec::from_persona(&p).unwrap();
  let recent = SystemTime::now() - Duration::from_secs(1);
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "hi",
   },
   recent,
   no_jitter,
  );
  assert!(matches!(d, Decision::SkipByMinInterval { .. }), "got {:?}", d);
 }

 #[test]
 fn explicit_conf_with_regex_match_scores_sum() {
  let conf = DecisionConf {
   threshold: 100.0,
   base_score: 30.0,
   jitter: 0.0,
   modulators: vec![
    DecisionModulator::RegexMatch {
     pattern: "ケルシー".into(),
     score: 80.0,
    },
    DecisionModulator::Always { score: 0.0 },
   ],
   min_interval_in_secs: None,
  };
  let spec = DecisionSpec::from_conf(&conf).unwrap();
  let hit = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "ケルシー、聞こえてる？",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(hit, Decision::Activate { .. }), "got {:?}", hit);

  let miss = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "今日は寒いね",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(miss, Decision::SkipByScore { .. }), "got {:?}", miss);
 }

 #[test]
 fn channel_match_adds_score_for_matching_source() {
  let conf = DecisionConf {
   threshold: 50.0,
   base_score: 0.0,
   jitter: 0.0,
   modulators: vec![DecisionModulator::ChannelMatch {
    channels: vec!["vip".into()],
    score: 60.0,
   }],
   min_interval_in_secs: None,
  };
  let spec = DecisionSpec::from_conf(&conf).unwrap();
  let matched = spec.evaluate(
   &DecisionInput {
    channel: "vip",
    latest_trigger_content: "…",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(matched, Decision::Activate { .. }));

  let other = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "…",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(other, Decision::SkipByScore { .. }));
 }

 #[test]
 fn regex_ignore_overrides_large_positive_score() {
  let conf = DecisionConf {
   threshold: 50.0,
   base_score: 0.0,
   jitter: 0.0,
   modulators: vec![
    DecisionModulator::Always { score: 1_000_000.0 },
    DecisionModulator::RegexIgnore { pattern: "^spam".into() },
   ],
   min_interval_in_secs: None,
  };
  let spec = DecisionSpec::from_conf(&conf).unwrap();
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "spam spam",
   },
   SystemTime::UNIX_EPOCH,
   no_jitter,
  );
  assert!(matches!(d, Decision::SkipByScore { .. }), "got {:?}", d);
 }

 #[test]
 fn silence_since_accumulates_score_proportional_to_excess() {
  let conf = DecisionConf {
   threshold: 50.0,
   base_score: 0.0,
   jitter: 0.0,
   modulators: vec![DecisionModulator::SilenceSince {
    threshold_secs: 10,
    score_per_sec: 5.0,
    max_score: 100.0,
   }],
   min_interval_in_secs: None,
  };
  let spec = DecisionSpec::from_conf(&conf).unwrap();

  // しきい値未満 → スコア 0 → Skip
  let recent = SystemTime::now() - Duration::from_secs(5);
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "",
   },
   recent,
   no_jitter,
  );
  assert!(matches!(d, Decision::SkipByScore { .. }), "got {:?}", d);

  // しきい値 +20 秒（計 30 秒）→ 20 * 5 = 100 → threshold 50 を突破
  let long_silent = SystemTime::now() - Duration::from_secs(30);
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "",
   },
   long_silent,
   no_jitter,
  );
  assert!(matches!(d, Decision::Activate { .. }), "got {:?}", d);
 }

 #[test]
 fn silence_since_clips_at_max_score() {
  let conf = DecisionConf {
   threshold: 10_000.0,
   base_score: 0.0,
   jitter: 0.0,
   modulators: vec![DecisionModulator::SilenceSince {
    threshold_secs: 0,
    score_per_sec: 1_000_000.0,
    max_score: 50.0,
   }],
   min_interval_in_secs: None,
  };
  let spec = DecisionSpec::from_conf(&conf).unwrap();
  let ancient = SystemTime::now() - Duration::from_secs(3600);
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "",
   },
   ancient,
   no_jitter,
  );
  // max_score = 50.0 でクリップされるので threshold 10000 は突破できない
  match d {
   Decision::SkipByScore { score, .. } => assert!((score - 50.0).abs() < f64::EPSILON, "got {}", score),
   other => panic!("expected SkipByScore with 50.0, got {:?}", other),
  }
 }

 #[test]
 fn jitter_supplier_is_consulted_with_configured_value() {
  let conf = DecisionConf {
   threshold: 50.0,
   base_score: 40.0,
   jitter: 25.0,
   modulators: vec![],
   min_interval_in_secs: None,
  };
  let spec = DecisionSpec::from_conf(&conf).unwrap();
  let d = spec.evaluate(
   &DecisionInput {
    channel: "user",
    latest_trigger_content: "",
   },
   SystemTime::UNIX_EPOCH,
   |j| {
    assert!((j - 25.0).abs() < f64::EPSILON);
    15.0
   },
  );
  assert!(matches!(d, Decision::Activate { .. }), "got {:?}", d);
 }
}
