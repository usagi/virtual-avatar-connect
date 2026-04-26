//! OS 標準 TTS ドライバ（Windows SAPI / macOS NSSpeech / Linux espeak など、`tts` crate 経由）。
//!
//! ## プラットフォーム挙動
//!
//! - **Windows**: `tts.synthesize()` で WAV bytes を取得し、VAC 共有 `audio_sink` に append する。
//!   その他エンジンと同じ再生経路になり、`speech_floor` や BGM とのミキシングも効く。
//! - **非 Windows**: `tts.synthesize()` が未サポートのため、`tts.speak()`（ブロッキング非同期）で
//!   再生させ、`is_speaking()` をポーリングして終了待ち。`audio_sink` は使わない。
//!
//! ## `voice` の構文
//!
//! - 非空文字列: OS 音声の **名前に対する部分一致**（V1 互換）。例: `"Haruka"`, `"Microsoft Zira"`。
//! - 空文字: OS デフォルト音声。
//!
//! ## 正規化パラメータのマッピング
//!
//! `tts` crate は `min_rate()` / `normal_rate()` / `max_rate()` を OS ごとに持つ。
//! 以下のスケーリング関数で正規化値を OS 固有範囲にマップする:
//!
//! - `req.speed`  (1.0=標準): `>1.0` → `[normal, max]`、`<1.0` → `[min, normal]` の線形補間。
//! - `req.pitch`  (0.0=標準): `>=0` → `[normal, max]`、`<0` → `[min, normal]`。
//! - `req.volume` (1.0=標準): `>1.0` → `[normal, max]`、`<1.0` → `[min, normal]`（1.0 以上は clamp）。

use super::super::driver::{maybe_save_wav, AudioContext, TtsDriver, TtsError, TtsOutcome, TtsParamSchema, TtsRequest};
use async_trait::async_trait;

pub struct OsDriver;

fn scale_rate_like(norm: f64, min: f32, normal: f32, max: f32) -> f32 {
	let n = norm as f32;
	if n >= 1.0 {
		let t = (n - 1.0).clamp(0.0, 1.0);
		normal + (max - normal) * t
	} else {
		let t = n.clamp(0.0, 1.0);
		min + (normal - min) * t
	}
}

fn scale_pitch_like(norm: f64, min: f32, normal: f32, max: f32) -> f32 {
	let n = norm as f32;
	if n >= 0.0 {
		normal + (max - normal) * n.clamp(0.0, 1.0)
	} else {
		normal + (normal - min) * n.clamp(-1.0, 0.0)
	}
}

#[async_trait]
impl TtsDriver for OsDriver {
	fn name(&self) -> &'static str {
		"os"
	}

	fn params_schema(&self) -> TtsParamSchema {
		TtsParamSchema::default()
	}

	async fn speak(&self, req: TtsRequest, audio: &AudioContext<'_>) -> Result<TtsOutcome, TtsError> {
		let mut tts = tts::Tts::default().map_err(|e| TtsError::Config(format!("tts::Tts::default: {e}")))?;

		if !req.voice.is_empty() {
			let voices = tts.voices().map_err(|e| TtsError::Config(format!("voices(): {e}")))?;
			if let Some(v) = voices.iter().find(|v| v.name().contains(&req.voice)) {
				tts.set_voice(v).map_err(|e| TtsError::Config(format!("set_voice: {e}")))?;
			} else {
				log::warn!("[tts.os] voice '{}' が見つかりませんでした。OS デフォルトを使用します。", req.voice);
			}
		}

		let rate = scale_rate_like(req.speed, tts.min_rate(), tts.normal_rate(), tts.max_rate());
		tts.set_rate(rate).map_err(|e| TtsError::Config(format!("set_rate: {e}")))?;

		let pitch = scale_pitch_like(req.pitch, tts.min_pitch(), tts.normal_pitch(), tts.max_pitch());
		tts.set_pitch(pitch).map_err(|e| TtsError::Config(format!("set_pitch: {e}")))?;

		let volume = scale_rate_like(req.volume, tts.min_volume(), tts.normal_volume(), tts.max_volume());
		tts.set_volume(volume).map_err(|e| TtsError::Config(format!("set_volume: {e}")))?;

		#[cfg(target_os = "windows")]
		{
			let wav = tts
				.synthesize(req.text)
				.map_err(|e| TtsError::Synthesis(format!("synthesize: {e}")))?;
			let played = audio.play_wav(wav.clone()).await?;
			let path = maybe_save_wav(&wav, &req.save_path).await?;
			Ok(TtsOutcome { played, audio_path: path })
		}

		#[cfg(not(target_os = "windows"))]
		{
			let _ = audio; // 非 Windows は自前で再生するため未使用
			if !req.save_path.is_empty() {
				log::warn!("[tts.os] save_path は非 Windows では未対応です。無視します。");
			}
			tts.speak(req.text, false).map_err(|e| TtsError::Synthesis(format!("speak: {e}")))?;
			// 終了待ち
			loop {
				match tts.is_speaking() {
					Ok(true) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
					Ok(false) => break,
					Err(e) => return Err(TtsError::Playback(format!("is_speaking: {e}"))),
				}
			}
			Ok(TtsOutcome {
				played: true,
				audio_path: String::new(),
			})
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rate_scaling_at_boundaries() {
		// min=0, normal=1, max=2
		assert!((scale_rate_like(1.0, 0.0, 1.0, 2.0) - 1.0).abs() < 1e-6);
		assert!((scale_rate_like(2.0, 0.0, 1.0, 2.0) - 2.0).abs() < 1e-6);
		assert!((scale_rate_like(0.0, 0.0, 1.0, 2.0) - 0.0).abs() < 1e-6);
		assert!((scale_rate_like(1.5, 0.0, 1.0, 2.0) - 1.5).abs() < 1e-6);
		// clamp
		assert!((scale_rate_like(5.0, 0.0, 1.0, 2.0) - 2.0).abs() < 1e-6);
		assert!((scale_rate_like(-1.0, 0.0, 1.0, 2.0) - 0.0).abs() < 1e-6);
	}

	#[test]
	fn pitch_scaling_at_boundaries() {
		// min=-1, normal=0, max=1
		assert!((scale_pitch_like(0.0, -1.0, 0.0, 1.0) - 0.0).abs() < 1e-6);
		assert!((scale_pitch_like(1.0, -1.0, 0.0, 1.0) - 1.0).abs() < 1e-6);
		assert!((scale_pitch_like(-1.0, -1.0, 0.0, 1.0) - (-1.0)).abs() < 1e-6);
		assert!((scale_pitch_like(0.5, -1.0, 0.0, 1.0) - 0.5).abs() < 1e-6);
	}
}
