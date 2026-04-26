//! VoicePeak TTS ドライバ（CLI `voicepeak[.exe]` spawn 実装）。
//!
//! VoicePeak は HTTP/TCP API を公開していないため、付属の CLI を `tokio::process::Command`
//! で呼び出し、出力 WAV を読んで VAC 共有 `audio_sink` に append する。
//!
//! ## 解決順序（実行ファイルパス）
//!
//! 1. 非空の `req.endpoint` を exe パスとして使用（`tts.speak` が VoicePeak 用に注入する想定。
//!    空のときはノード側で `[voicepeak]` + OS 既定を `endpoint` に詰めてから到達する）。
//! 2. **非推奨**: `extra.executable` が非空で `endpoint` が空のときのみその値を使い、1 回だけ `warn!`。
//! 3. どちらも無ければ PATH 上の `voicepeak` / `voicepeak.exe` に委ねる（ヘッドレス等で `state_handle` が無い場合）。
//!
//! ## CLI とパラメータ対応
//!
//! `voicepeak -s "text" -o out.wav [-n <narrator>] [-e "<csv>"] [--speed N] [--pitch N]`
//!
//! - `req.voice` → `-n <narrator>`（空なら omit、CLI は直前の GUI 既定を使用）
//! - `req.speed`  (1.0 = 標準): `--speed <50..200>` に線形変換（`100 * req.speed` を clamp）
//! - `req.pitch`  (0.0 = 標準): `--pitch <-300..300>` に線形変換（`300 * req.pitch` を clamp）
//! - `req.volume` (1.0 = 標準): **VoicePeak CLI は音量オプションを持たない**。1.0 以外が
//!   指定されたときは WARN ログを 1 行出して無視する（音量調整は rodio 側で post-process）。
//! - `extra.emotion: String` → `-e "<csv>"` そのまま（例: `"happy=50,angry=10"`）。
//! - `extra.narrator: String` → `-n` 上書き（`voice` より優先）。
//! - `extra.speed_raw: Int` → `--speed` を正規化せず直接指定。
//! - `extra.pitch_raw: Int` → `--pitch` を正規化せず直接指定。
//!
//! ## 再生経路
//!
//! CLI 終了 → 出力 WAV を読む → `audio.play_wav` → `maybe_save_wav(&wav, &req.save_path)`
//! → 一時ファイル削除、の順。60 秒で timeout し、`TtsError::Synthesis` に畳む。

use super::super::driver::{
	extra_i64, extra_str, maybe_save_wav, resolve_save_path, AudioContext, TtsDriver, TtsError, TtsOutcome,
	TtsParamEntry, TtsParamSchema, TtsParamType, TtsRequest,
};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::process::Command;

static VOICEPEAK_EXECUTABLE_DEPRECATION_WARNED: AtomicBool = AtomicBool::new(false);

pub struct VoicepeakDriver;

const DEFAULT_EXECUTABLE: &str = if cfg!(windows) { "voicepeak.exe" } else { "voicepeak" };
const SPAWN_TIMEOUT: Duration = Duration::from_secs(60);

fn map_speed(req_speed: f64) -> i32 {
	let v = (req_speed * 100.0).round() as i64;
	v.clamp(50, 200) as i32
}

fn map_pitch(req_pitch: f64) -> i32 {
	let v = (req_pitch * 300.0).round() as i64;
	v.clamp(-300, 300) as i32
}

/// CLI 起動に使う executable パスを `endpoint`（および後方互換の `extra.executable`）から解決する。
fn resolve_executable(req: &TtsRequest) -> String {
	if !req.endpoint.is_empty() {
		return req.endpoint.clone();
	}
	if let Some(exe) = extra_str(&req.extra, "executable").filter(|s| !s.is_empty()) {
		if !VOICEPEAK_EXECUTABLE_DEPRECATION_WARNED.swap(true, Ordering::Relaxed) {
			log::warn!(
				"[tts.voicepeak] extra.executable is deprecated; set the flowgraph `tts.speak` `endpoint` input (or `[voicepeak].path` in conf) for the voicepeak CLI path."
			);
		}
		return exe.to_string();
	}
	DEFAULT_EXECUTABLE.to_string()
}

/// CLI 引数を組み立てる。テスト用に抽出。
/// `output_path` は呼び出し側で決めた一時 WAV のパス。
fn build_args(req: &TtsRequest, output_path: &str) -> Vec<String> {
	let mut args: Vec<String> = Vec::with_capacity(10);
	args.push("-s".into());
	args.push(req.text.clone());
	args.push("-o".into());
	args.push(output_path.to_string());

	let narrator = extra_str(&req.extra, "narrator").filter(|s| !s.is_empty()).map(|s| s.to_string()).or_else(|| {
		if req.voice.is_empty() {
			None
		} else {
			Some(req.voice.clone())
		}
	});
	if let Some(n) = narrator {
		args.push("-n".into());
		args.push(n);
	}

	if let Some(emo) = extra_str(&req.extra, "emotion").filter(|s| !s.is_empty()) {
		args.push("-e".into());
		args.push(emo.to_string());
	}

	let speed = extra_i64(&req.extra, "speed_raw").map(|v| v.clamp(50, 200) as i32).unwrap_or_else(|| map_speed(req.speed));
	if speed != 100 {
		args.push("--speed".into());
		args.push(speed.to_string());
	}

	let pitch =
		extra_i64(&req.extra, "pitch_raw").map(|v| v.clamp(-300, 300) as i32).unwrap_or_else(|| map_pitch(req.pitch));
	if pitch != 0 {
		args.push("--pitch".into());
		args.push(pitch.to_string());
	}

	args
}

/// 一時 WAV ファイル用のユニークなパスを生成する。
fn temp_wav_path() -> PathBuf {
	let ts = jiff::Timestamp::now().as_nanosecond();
	let name = format!("vac-voicepeak-{ts}-{pid}.wav", pid = std::process::id());
	std::env::temp_dir().join(name)
}

#[async_trait]
impl TtsDriver for VoicepeakDriver {
	fn name(&self) -> &'static str {
		"voicepeak"
	}

	fn params_schema(&self) -> TtsParamSchema {
		TtsParamSchema {
			entries: vec![
				TtsParamEntry { key: "narrator", ty: TtsParamType::String, description: "voice 入力の代わりにナレーターを直接指定（voice より優先）" },
				TtsParamEntry { key: "emotion", ty: TtsParamType::String, description: "CLI -e に渡す感情 CSV（例: \"happy=50,angry=10\"）" },
				TtsParamEntry { key: "speed_raw", ty: TtsParamType::Int, description: "--speed を正規化せず直接指定（50..=200）" },
				TtsParamEntry { key: "pitch_raw", ty: TtsParamType::Int, description: "--pitch を正規化せず直接指定（-300..=300）" },
			],
		}
	}

	async fn speak(&self, req: TtsRequest, audio: &AudioContext<'_>) -> Result<TtsOutcome, TtsError> {
		if (req.volume - 1.0).abs() > f64::EPSILON {
			log::warn!(
				"[tts.voicepeak] volume={} が指定されましたが、VoicePeak CLI は音量オプションを持たないため無視します。",
				req.volume
			);
		}

		let exe = resolve_executable(&req);
		let out_path = temp_wav_path();
		let out_str = out_path.to_string_lossy().to_string();
		let args = build_args(&req, &out_str);

		let spawn_result =
			tokio::time::timeout(SPAWN_TIMEOUT, Command::new(&exe).args(&args).output()).await.map_err(|_| {
				TtsError::Synthesis(format!("'{exe}' timed out after {}s", SPAWN_TIMEOUT.as_secs()))
			})?;
		let output = spawn_result.map_err(|e| {
			// `Command::output` が失敗 = executable 起動失敗（NotFound 等）
			TtsError::Io(format!("spawn '{exe}' failed: {e}"))
		})?;

		if !output.status.success() {
			let _ = tokio::fs::remove_file(&out_path).await;
			let stderr = String::from_utf8_lossy(&output.stderr);
			let stdout = String::from_utf8_lossy(&output.stdout);
			return Err(TtsError::Synthesis(format!(
				"'{exe}' exited with {}: stderr={}, stdout={}",
				output.status,
				stderr.trim(),
				stdout.trim()
			)));
		}

		let wav = match tokio::fs::read(&out_path).await {
			Ok(b) => b,
			Err(e) => {
				let _ = tokio::fs::remove_file(&out_path).await;
				return Err(TtsError::Io(format!("read voicepeak output '{}': {e}", out_path.display())));
			},
		};
		let _ = tokio::fs::remove_file(&out_path).await;

		if wav.is_empty() {
			return Err(TtsError::Synthesis(format!("'{exe}' produced empty WAV ({} args)", args.len())));
		}

		let played = audio.play_wav(wav.clone()).await?;
		let saved = maybe_save_wav(&wav, &req.save_path).await?;
		let audio_path = if saved.is_empty() { resolve_save_path(&req.save_path) } else { saved };
		Ok(TtsOutcome { played, audio_path: if req.save_path.is_empty() { String::new() } else { audio_path } })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::socket::SocketValue;
	use std::collections::BTreeMap;

	fn base_req() -> TtsRequest {
		TtsRequest {
			text: "こんにちは".into(),
			voice: String::new(),
			speed: 1.0,
			pitch: 0.0,
			volume: 1.0,
			endpoint: String::new(),
			save_path: String::new(),
			extra: BTreeMap::new(),
		}
	}

	#[test]
	fn speed_mapping() {
		assert_eq!(map_speed(1.0), 100);
		assert_eq!(map_speed(2.0), 200);
		assert_eq!(map_speed(0.5), 50);
		assert_eq!(map_speed(0.1), 50); // clamp
		assert_eq!(map_speed(5.0), 200); // clamp
	}

	#[test]
	fn pitch_mapping() {
		assert_eq!(map_pitch(0.0), 0);
		assert_eq!(map_pitch(1.0), 300);
		assert_eq!(map_pitch(-1.0), -300);
		assert_eq!(map_pitch(0.5), 150);
		assert_eq!(map_pitch(-5.0), -300); // clamp
	}

	#[test]
	fn build_args_minimal_skips_defaults() {
		let req = base_req();
		let args = build_args(&req, "/tmp/out.wav");
		// 既定パラメータのときは -n / -e / --speed / --pitch は省略
		assert_eq!(args, vec!["-s", "こんにちは", "-o", "/tmp/out.wav"]);
	}

	#[test]
	fn build_args_with_voice_emotion_speed_pitch() {
		let mut req = base_req();
		req.voice = "Japanese Female 1".into();
		req.speed = 1.2;
		req.pitch = -0.3;
		req.extra.insert("emotion".into(), SocketValue::String("happy=50,angry=10".into()));
		let args = build_args(&req, "/tmp/out.wav");
		assert!(args.contains(&"-n".into()));
		assert!(args.contains(&"Japanese Female 1".into()));
		assert!(args.contains(&"-e".into()));
		assert!(args.contains(&"happy=50,angry=10".into()));
		assert!(args.contains(&"--speed".into()));
		assert!(args.contains(&"120".into()));
		assert!(args.contains(&"--pitch".into()));
		assert!(args.contains(&"-90".into()));
	}

	#[test]
	fn build_args_extra_overrides_voice_and_rates() {
		let mut req = base_req();
		req.voice = "WillBeOverridden".into();
		req.extra.insert("narrator".into(), SocketValue::String("Alt Narrator".into()));
		req.extra.insert("speed_raw".into(), SocketValue::Int(175));
		req.extra.insert("pitch_raw".into(), SocketValue::Int(-250));
		let args = build_args(&req, "/tmp/out.wav");
		// narrator 優先
		assert!(args.contains(&"Alt Narrator".into()));
		assert!(!args.contains(&"WillBeOverridden".into()));
		// raw 値が使われる
		assert!(args.contains(&"175".into()));
		assert!(args.contains(&"-250".into()));
	}

	#[test]
	fn resolve_executable_endpoint_wins_over_deprecated_executable() {
		let mut req = base_req();
		req.endpoint = "C:/from-endpoint.exe".into();
		req.extra.insert("executable".into(), SocketValue::String("C:/from-extra.exe".into()));
		assert_eq!(resolve_executable(&req), "C:/from-endpoint.exe");
	}

	#[test]
	fn resolve_executable_deprecated_extra_when_endpoint_empty() {
		let mut req = base_req();
		req.extra.insert("executable".into(), SocketValue::String("C:/from-extra.exe".into()));
		assert_eq!(resolve_executable(&req), "C:/from-extra.exe");
	}

	#[test]
	fn resolve_executable_falls_back_to_default_when_empty() {
		let mut req = base_req();
		assert_eq!(resolve_executable(&req), DEFAULT_EXECUTABLE);
		req.endpoint = "C:/from-endpoint.exe".into();
		assert_eq!(resolve_executable(&req), "C:/from-endpoint.exe");
	}

	#[tokio::test]
	async fn unreachable_executable_yields_io_error() {
		let driver = VoicepeakDriver;
		let mut req = base_req();
		// 存在しないパスを endpoint にして、TtsError::Io に落ちることを確認。
		req.endpoint = "C:/vac-nonexistent-voicepeak-xyz-12345.exe".into();
		let audio = AudioContext { sink: None };
		let err = driver.speak(req, &audio).await.unwrap_err();
		// OS によって Io か Synthesis（タイムアウト）に畳まれるが、少なくとも非致命的に畳まれる
		assert!(matches!(err, TtsError::Io(_) | TtsError::Synthesis(_)), "got: {err:?}");
	}
}
