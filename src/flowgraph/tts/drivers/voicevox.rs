//! VOICEVOX エンジン TTS ドライバ（VOICEVOX 互換 HTTP API）。
//!
//! `crate::processor::voicevox_engine::synthesize_wav` を再利用して `/audio_query` → `/synthesis`
//! を叩く。`AivisSpeechDriver` も同じ実装をデフォルトエンドポイントだけ差し替えて使用する。
//!
//! ## `voice` の構文
//!
//! - `"12345"`  → そのまま VOICEVOX の **グローバル `style_id`**（`GET /speakers` に出てくる `styles[].id`）。
//! - `"uuid:N"` → 話者 UUID とその内部の **N 番目（0 始まり）のスタイル**。`/speakers` を引いて解決する。
//! - 空 → エラー（`Config` 扱い）。
//!
//! ## 正規化パラメータのマッピング
//!
//! - `req.speed`  → `speed_scale`（1.0=標準）
//! - `req.pitch`  → `pitch_scale`（0.0=標準。VOICEVOX は ±0.15 程度が実用域）
//! - `req.volume` → `volume_scale`
//! - `extra.intonation_scale` (Float, default 1.0)
//! - `extra.pre_phoneme_length` (Float, default 0.1)
//! - `extra.post_phoneme_length` (Float, default 0.1)
//! - `extra.output_sampling_rate` (Int, default 24000)
//! - `extra.save_path` → 省略（ノード側の `save_path` を使う）

use super::super::driver::{
 extra_f64, extra_i64, maybe_save_wav, AudioContext, TtsDriver, TtsError, TtsOutcome, TtsParamEntry,
 TtsParamSchema, TtsParamType, TtsRequest,
};
use async_trait::async_trait;

pub(super) async fn speak_voicevox_like(
 engine_base_default: &str,
 req: &TtsRequest,
 audio: &AudioContext<'_>,
) -> Result<TtsOutcome, TtsError> {
 let engine_base = if req.endpoint.is_empty() { engine_base_default } else { req.endpoint.as_str() };

 if req.voice.trim().is_empty() {
  return Err(TtsError::Config(
   "voice が空です。VOICEVOX 系はグローバル style_id もしくは 'uuid:local_index' を指定してください".into(),
  ));
 }

 let speaker = resolve_speaker(engine_base, &req.voice).await?;

 let speed_scale = req.speed;
 let volume_scale = req.volume;
 let pitch_scale = req.pitch;
 let intonation_scale = extra_f64(&req.extra, "intonation_scale").unwrap_or(1.0);
 let pre_phoneme_length = extra_f64(&req.extra, "pre_phoneme_length").unwrap_or(0.1);
 let post_phoneme_length = extra_f64(&req.extra, "post_phoneme_length").unwrap_or(0.1);
 let output_sampling_rate = extra_i64(&req.extra, "output_sampling_rate").unwrap_or(24000).max(8000) as u32;

 let wav = crate::processor::voicevox_engine::synthesize_wav(
  engine_base,
  speaker,
  req.text.clone(),
  speed_scale,
  volume_scale,
  pitch_scale,
  intonation_scale,
  pre_phoneme_length,
  post_phoneme_length,
  output_sampling_rate,
 )
 .await
 .map_err(TtsError::Synthesis)?;

 let played = audio.play_wav(wav.clone()).await?;
 let path = maybe_save_wav(&wav, &req.save_path).await?;
 Ok(TtsOutcome { played, audio_path: path })
}

/// `voice` を `"uuid:N"` / `"12345"` のどちらかとして解釈し、グローバル style_id を返す。
async fn resolve_speaker(engine_base: &str, voice: &str) -> Result<i64, TtsError> {
 let v = voice.trim();
 if let Some((uuid, rest)) = v.split_once(':') {
  let local_index = rest.trim().parse::<usize>().map_err(|_| {
   TtsError::Config(format!(
    "voice '{v}' の ':' 以降はローカルスタイル番号（0 始まりの整数）である必要があります"
   ))
  })?;
  crate::processor::voicevox_engine::fetch_global_style_id(engine_base, uuid.trim(), local_index, "flowgraph.tts")
   .await
   .map_err(|e| TtsError::Synthesis(format!("{e}")))
 } else {
  v.parse::<i64>().map_err(|_| TtsError::Config(format!("voice '{v}' を style_id (整数) or 'uuid:N' として解釈できません")))
 }
}

pub struct VoicevoxDriver;

const VOICEVOX_DEFAULT: &str = "http://127.0.0.1:50021";

#[async_trait]
impl TtsDriver for VoicevoxDriver {
 fn name(&self) -> &'static str {
  "voicevox"
 }
 fn params_schema(&self) -> TtsParamSchema {
  voicevox_like_schema()
 }
 async fn speak(&self, req: TtsRequest, audio: &AudioContext<'_>) -> Result<TtsOutcome, TtsError> {
  speak_voicevox_like(VOICEVOX_DEFAULT, &req, audio).await
 }
}

pub(super) fn voicevox_like_schema() -> TtsParamSchema {
 TtsParamSchema {
  entries: vec![
   TtsParamEntry { key: "intonation_scale", ty: TtsParamType::Float, description: "VOICEVOX intonationScale (default 1.0)" },
   TtsParamEntry { key: "pre_phoneme_length", ty: TtsParamType::Float, description: "prePhonemeLength (default 0.1)" },
   TtsParamEntry { key: "post_phoneme_length", ty: TtsParamType::Float, description: "postPhonemeLength (default 0.1)" },
   TtsParamEntry { key: "output_sampling_rate", ty: TtsParamType::Int, description: "outputSamplingRate (default 24000)" },
  ],
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 async fn voice_parse_global_id() {
  // engine 不達でもパース段階のエラーなら Config
  let r = resolve_speaker("http://127.0.0.1:1", "42").await;
  assert_eq!(r.unwrap(), 42);
 }

 #[tokio::test]
 async fn voice_parse_uuid_format_syntax_error() {
  let r = resolve_speaker("http://127.0.0.1:1", "uuid:not_a_number").await;
  assert!(matches!(r, Err(TtsError::Config(_))));
 }

 #[tokio::test]
 async fn voice_parse_garbage() {
  let r = resolve_speaker("http://127.0.0.1:1", "not_a_number").await;
  assert!(matches!(r, Err(TtsError::Config(_))));
 }

 #[tokio::test]
 async fn unreachable_engine_is_synthesis_error() {
  let d = VoicevoxDriver;
  let req = TtsRequest {
   text: "test".into(),
   voice: "1".into(),
   speed: 1.0,
   pitch: 0.0,
   volume: 1.0,
   endpoint: "http://127.0.0.1:1".into(),
   save_path: String::new(),
   extra: std::collections::BTreeMap::new(),
  };
  let audio = AudioContext { sink: None };
  let err = d.speak(req, &audio).await.unwrap_err();
  assert!(matches!(err, TtsError::Synthesis(_)));
 }
}
