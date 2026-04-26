//! 棒読みちゃん TTS ドライバ（TCP プロトコル実装）。
//!
//! V1 は `RemoteTalk.exe` CLI を spawn していたが、Flowgraph 版では **TCP プロトコル（port 50001）を採用**。
//! 外部 exe のパス設定が不要になり、ネットワーク越しの別マシン上 Bouyomichan にも接続できる。
//!
//! ## プロトコル
//!
//! 棒読みちゃんの TCP 読み上げリクエストバイナリフォーマット（全て Little Endian）:
//!
//! ```text
//! iCommand  i16   1 (Talk)
//! iSpeed    i16   -1=default | 50..=300
//! iTone     i16   -1=default | 50..=200
//! iVolume   i16   -1=default | 0..=100
//! iVoice    i16   0..=8
//! bCode     u8    0=UTF-8, 1=UTF-16LE
//! iLength   i32   text byte length
//! bText     u8[]  text bytes
//! ```
//!
//! ## 正規化パラメータのマッピング
//!
//! - `req.speed` (1.0=標準) → `iSpeed = clamp((req.speed * 100.0) as i16, 50, 300)`
//! - `req.pitch` (0.0=標準) → `iTone  = clamp((100.0 + req.pitch * 50.0) as i16, 50, 200)`
//! - `req.volume` (1.0=標準) → `iVolume = clamp((req.volume * 100.0) as i16, 0, 100)`
//! - `req.voice` は `"0"`〜`"8"` の数値文字列。空なら 0（デフォルト）。
//! - `req.endpoint` は `host:port`。空なら `127.0.0.1:50001`。
//! - `extra.speed_raw`, `extra.tone_raw`, `extra.volume_raw`, `extra.voice_raw` で
//!   正規化を迂回して棒読みちゃんの生値を直接指定できる（エキスパート用）。

use super::super::driver::{
	extra_i64, AudioContext, TtsDriver, TtsError, TtsOutcome, TtsParamEntry, TtsParamSchema, TtsParamType, TtsRequest,
};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

pub struct BouyomichanDriver;

const DEFAULT_ENDPOINT: &str = "127.0.0.1:50001";

fn map_speed(req_speed: f64) -> i16 {
	let v = (req_speed * 100.0).round() as i64;
	v.clamp(50, 300) as i16
}

fn map_tone(req_pitch: f64) -> i16 {
	let v = (100.0 + req_pitch * 50.0).round() as i64;
	v.clamp(50, 200) as i16
}

fn map_volume(req_volume: f64) -> i16 {
	let v = (req_volume * 100.0).round() as i64;
	v.clamp(0, 100) as i16
}

fn parse_voice(req_voice: &str) -> i16 {
	req_voice.trim().parse::<i16>().ok().map(|v| v.clamp(0, 8)).unwrap_or(0)
}

fn build_packet(speed: i16, tone: i16, volume: i16, voice: i16, text: &str) -> Vec<u8> {
	let text_bytes = text.as_bytes();
	let text_len = text_bytes.len() as i32;
	let mut buf = Vec::with_capacity(15 + text_bytes.len());
	buf.extend_from_slice(&1i16.to_le_bytes()); // iCommand = 1 (Talk)
	buf.extend_from_slice(&speed.to_le_bytes());
	buf.extend_from_slice(&tone.to_le_bytes());
	buf.extend_from_slice(&volume.to_le_bytes());
	buf.extend_from_slice(&voice.to_le_bytes());
	buf.push(0u8); // bCode = UTF-8
	buf.extend_from_slice(&text_len.to_le_bytes());
	buf.extend_from_slice(text_bytes);
	buf
}

#[async_trait]
impl TtsDriver for BouyomichanDriver {
	fn name(&self) -> &'static str {
		"bouyomichan"
	}

	fn params_schema(&self) -> TtsParamSchema {
		TtsParamSchema {
			entries: vec![
				TtsParamEntry {
					key: "speed_raw",
					ty: TtsParamType::Int,
					description: "iSpeed 直接指定 (-1 or 50..=300)",
				},
				TtsParamEntry {
					key: "tone_raw",
					ty: TtsParamType::Int,
					description: "iTone 直接指定 (-1 or 50..=200)",
				},
				TtsParamEntry {
					key: "volume_raw",
					ty: TtsParamType::Int,
					description: "iVolume 直接指定 (-1 or 0..=100)",
				},
				TtsParamEntry {
					key: "voice_raw",
					ty: TtsParamType::Int,
					description: "iVoice 直接指定 (0..=8)",
				},
			],
		}
	}

	async fn speak(&self, req: TtsRequest, _audio: &AudioContext<'_>) -> Result<TtsOutcome, TtsError> {
		let endpoint = if req.endpoint.is_empty() {
			DEFAULT_ENDPOINT
		} else {
			req.endpoint.as_str()
		};

		let speed = extra_i64(&req.extra, "speed_raw")
			.map(|v| v.clamp(-1, 300) as i16)
			.unwrap_or_else(|| map_speed(req.speed));
		let tone = extra_i64(&req.extra, "tone_raw")
			.map(|v| v.clamp(-1, 200) as i16)
			.unwrap_or_else(|| map_tone(req.pitch));
		let volume = extra_i64(&req.extra, "volume_raw")
			.map(|v| v.clamp(-1, 100) as i16)
			.unwrap_or_else(|| map_volume(req.volume));
		let voice = extra_i64(&req.extra, "voice_raw")
			.map(|v| v.clamp(0, 8) as i16)
			.unwrap_or_else(|| parse_voice(&req.voice));

		let packet = build_packet(speed, tone, volume, voice, &req.text);

		let mut stream = tokio::time::timeout(std::time::Duration::from_secs(5), TcpStream::connect(endpoint))
			.await
			.map_err(|_| TtsError::Network(format!("connect timeout to {endpoint}")))?
			.map_err(|e| TtsError::Network(format!("connect {endpoint}: {e}")))?;

		tokio::time::timeout(std::time::Duration::from_secs(5), stream.write_all(&packet))
			.await
			.map_err(|_| TtsError::Network("write timeout".into()))?
			.map_err(|e| TtsError::Network(format!("write: {e}")))?;
		stream.shutdown().await.ok();

		// Bouyomichan は合成後の音声を**自身で再生**する。VAC 側の audio_sink は使わない。
		// save_path も意味を持たない（WAV が返ってこない）ので空を返す。
		Ok(TtsOutcome {
			played: true,
			audio_path: String::new(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn speed_pitch_volume_mapping() {
		assert_eq!(map_speed(1.0), 100);
		assert_eq!(map_speed(2.0), 200);
		assert_eq!(map_speed(0.5), 50);
		assert_eq!(map_speed(0.1), 50); // clamp
		assert_eq!(map_speed(4.0), 300); // clamp
		assert_eq!(map_tone(0.0), 100);
		assert_eq!(map_tone(1.0), 150);
		assert_eq!(map_tone(-1.0), 50);
		assert_eq!(map_tone(10.0), 200); // clamp
		assert_eq!(map_volume(1.0), 100);
		assert_eq!(map_volume(0.0), 0);
		assert_eq!(map_volume(2.0), 100); // clamp
	}

	#[test]
	fn parse_voice_handles_edge_cases() {
		assert_eq!(parse_voice(""), 0);
		assert_eq!(parse_voice("3"), 3);
		assert_eq!(parse_voice("99"), 8); // clamp
		assert_eq!(parse_voice("abc"), 0); // parse fail → 0
	}

	#[test]
	fn build_packet_header_size_and_layout() {
		let p = build_packet(100, 100, 50, 1, "こんにちは");
		// iCommand(2) + iSpeed(2) + iTone(2) + iVolume(2) + iVoice(2) + bCode(1) + iLength(4) = 15
		assert_eq!(p.len(), 15 + "こんにちは".len());
		assert_eq!(&p[0..2], &1i16.to_le_bytes());
		assert_eq!(&p[2..4], &100i16.to_le_bytes());
		assert_eq!(p[10], 0u8); // UTF-8
		let want_len = ("こんにちは".len() as i32).to_le_bytes();
		assert_eq!(&p[11..15], &want_len);
	}

	#[tokio::test]
	async fn unreachable_endpoint_returns_network_error() {
		let d = BouyomichanDriver;
		let mut extra = std::collections::BTreeMap::new();
		let _ = &mut extra; // silence
		let req = TtsRequest {
			text: "test".into(),
			voice: "0".into(),
			speed: 1.0,
			pitch: 0.0,
			volume: 1.0,
			endpoint: "127.0.0.1:1".into(), // 常に拒否される
			save_path: String::new(),
			extra,
		};
		let audio = AudioContext { sink: None };
		let err = d.speak(req, &audio).await.unwrap_err();
		assert!(matches!(err, TtsError::Network(_)));
	}
}
