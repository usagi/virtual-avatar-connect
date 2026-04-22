//! CoeiroInk TTS ドライバ。
//!
//! V1 の `fix_conf` による「設定の暗黙補完＆ predict / synthesis 両対応」のカオスを捨て、
//! **`POST {endpoint}/v1/synthesis` に固定**した素直な実装に整理した。
//! エンドポイント既定は `http://127.0.0.1:50032`。
//!
//! ## `voice` の構文
//!
//! - `"uuid:style_id"` 形式のみ受理（CoeiroInk の仕様上、speaker_uuid は必須、style_id も数値で固定）。
//! - 空 or 形式不正は `Config` エラー。
//!
//! ## 正規化パラメータのマッピング
//!
//! - `req.speed`  → `speedScale` (1.0=標準)
//! - `req.pitch`  → `pitchScale` (0.0=標準)
//! - `req.volume` → `volumeScale`
//! - `extra.intonation_scale` / `extra.pre_phoneme_length` / `extra.post_phoneme_length` / `extra.output_sampling_rate`

use super::super::driver::{
 extra_f64, extra_i64, maybe_save_wav, AudioContext, TtsDriver, TtsError, TtsOutcome, TtsParamEntry, TtsParamSchema,
 TtsParamType, TtsRequest,
};
use async_trait::async_trait;
use serde::Serialize;

pub struct CoeiroinkDriver;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:50032";

#[derive(Serialize)]
#[allow(non_snake_case)]
struct SynthesisRequest<'a> {
 speakerUuid: &'a str,
 styleId: i64,
 text: &'a str,
 speedScale: f64,
 volumeScale: f64,
 pitchScale: f64,
 intonationScale: f64,
 prePhonemeLength: f64,
 postPhonemeLength: f64,
 outputSamplingRate: u32,
}

fn parse_voice(voice: &str) -> Result<(&str, i64), TtsError> {
 let v = voice.trim();
 let (uuid, rest) = v.split_once(':').ok_or_else(|| {
  TtsError::Config(format!("CoeiroInk の voice は 'uuid:style_id' 形式が必要です（got: '{v}'）"))
 })?;
 let style_id = rest.trim().parse::<i64>().map_err(|_| {
  TtsError::Config(format!("CoeiroInk の voice 末尾 style_id が整数ではありません: '{v}'"))
 })?;
 Ok((uuid.trim(), style_id))
}

#[async_trait]
impl TtsDriver for CoeiroinkDriver {
 fn name(&self) -> &'static str {
  "coeiroink"
 }
 fn params_schema(&self) -> TtsParamSchema {
  TtsParamSchema {
   entries: vec![
    TtsParamEntry { key: "intonation_scale", ty: TtsParamType::Float, description: "intonationScale (default 1.0)" },
    TtsParamEntry { key: "pre_phoneme_length", ty: TtsParamType::Float, description: "prePhonemeLength (default 0.1)" },
    TtsParamEntry { key: "post_phoneme_length", ty: TtsParamType::Float, description: "postPhonemeLength (default 0.1)" },
    TtsParamEntry { key: "output_sampling_rate", ty: TtsParamType::Int, description: "outputSamplingRate (default 48000)" },
   ],
  }
 }

 async fn speak(&self, req: TtsRequest, audio: &AudioContext<'_>) -> Result<TtsOutcome, TtsError> {
  let base = if req.endpoint.is_empty() { DEFAULT_ENDPOINT } else { req.endpoint.as_str() };
  let url = format!("{}/v1/synthesis", base.trim_end_matches('/'));

  let (uuid, style_id) = parse_voice(&req.voice)?;

  let body = SynthesisRequest {
   speakerUuid: uuid,
   styleId: style_id,
   text: &req.text,
   speedScale: req.speed,
   volumeScale: req.volume,
   pitchScale: req.pitch,
   intonationScale: extra_f64(&req.extra, "intonation_scale").unwrap_or(1.0),
   prePhonemeLength: extra_f64(&req.extra, "pre_phoneme_length").unwrap_or(0.1),
   postPhonemeLength: extra_f64(&req.extra, "post_phoneme_length").unwrap_or(0.1),
   outputSamplingRate: extra_i64(&req.extra, "output_sampling_rate").unwrap_or(48000).max(8000) as u32,
  };

  let client = reqwest::Client::builder()
   .timeout(std::time::Duration::from_secs(300))
   .build()
   .map_err(|e| TtsError::Network(e.to_string()))?;
  let resp = client
   .post(&url)
   .json(&body)
   .send()
   .await
   .map_err(|e| TtsError::Network(format!("POST {url}: {e}")))?;
  let status = resp.status();
  if !status.is_success() {
   let t = resp.text().await.unwrap_or_default();
   return Err(TtsError::Synthesis(format!("CoeiroInk HTTP {status}: {t}")));
  }
  let wav = resp.bytes().await.map_err(|e| TtsError::Network(format!("body: {e}")))?.to_vec();

  let played = audio.play_wav(wav.clone()).await?;
  let path = maybe_save_wav(&wav, &req.save_path).await?;
  Ok(TtsOutcome { played, audio_path: path })
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn parse_voice_ok() {
  let (u, s) = parse_voice("abc-def:3").unwrap();
  assert_eq!(u, "abc-def");
  assert_eq!(s, 3);
 }

 #[test]
 fn parse_voice_missing_colon_errors() {
  assert!(matches!(parse_voice("no_colon"), Err(TtsError::Config(_))));
 }

 #[test]
 fn parse_voice_non_integer_style() {
  assert!(matches!(parse_voice("uuid:abc"), Err(TtsError::Config(_))));
 }

 #[tokio::test]
 async fn unreachable_endpoint_yields_network_error() {
  let d = CoeiroinkDriver;
  let req = TtsRequest {
   text: "test".into(),
   voice: "abc:0".into(),
   speed: 1.0,
   pitch: 0.0,
   volume: 1.0,
   endpoint: "http://127.0.0.1:1".into(),
   save_path: String::new(),
   extra: std::collections::BTreeMap::new(),
  };
  let audio = AudioContext { sink: None };
  let err = d.speak(req, &audio).await.unwrap_err();
  assert!(matches!(err, TtsError::Network(_)));
 }
}
