//! AivisSpeech Engine TTS ドライバ。
//!
//! VOICEVOX 互換 HTTP API のため、実装は `voicevox` ドライバと共通。既定エンドポイントだけ差し替え。

use super::super::driver::{AudioContext, TtsDriver, TtsError, TtsOutcome, TtsParamSchema, TtsRequest};
use async_trait::async_trait;

pub struct AivisSpeechDriver;

const AIVIS_SPEECH_DEFAULT: &str = "http://127.0.0.1:10101";

#[async_trait]
impl TtsDriver for AivisSpeechDriver {
	fn name(&self) -> &'static str {
		"aivis_speech"
	}
	fn params_schema(&self) -> TtsParamSchema {
		super::voicevox::voicevox_like_schema()
	}
	async fn speak(&self, req: TtsRequest, audio: &AudioContext<'_>) -> Result<TtsOutcome, TtsError> {
		super::voicevox::speak_voicevox_like(AIVIS_SPEECH_DEFAULT, &req, audio).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn name_is_aivis_speech() {
		assert_eq!(AivisSpeechDriver.name(), "aivis_speech");
	}
}
