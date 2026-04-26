//! 《Voice》: Vosk（オフライン）でマイク入力を文字起こしする。
//!
//! 発話中は `partial_result` をログ出力し、無音で一区切りついたら確定して
//! [`crate::processor::voice::VoiceSink`] (`flowgraph.ingress.voice` ノード) に流す。
//!
//! V1 `[[processors]]` 経路は δ-9 Part E.2 で除去済み。本モジュールは Flowgraph bridge から
//! のみ呼ばれる。

use crate::bridges::voice::FlowgraphVoiceIngress;
use crate::flowgraph::node::TriggerHandle;
use crate::processor::voice::{VoiceIngress, VoiceSink};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use vosk::{CompleteResult, DecodingState, Model, Recognizer};

/// `flowgraph.ingress.voice` (engine=vosk) 経路の Vosk ワーカー起動。
///
/// `ingress.model_path` が空の場合はエラーログを出して None。
pub(crate) fn spawn_from_flowgraph(
	ingress: &FlowgraphVoiceIngress,
	tokio_handle: tokio::runtime::Handle,
	trigger: TriggerHandle,
) -> Option<VoiceIngress> {
	if ingress.model_path.trim().is_empty() {
		log::error!(
			"《Flowgraph/Voice》: ノード {} の `model_path` が空です。Vosk モデルディレクトリを指定してください。",
			ingress.node_id
		);
		return None;
	}
	let model_dir = ingress.model_path.clone();
	let node_id = ingress.node_id.clone();
	let fixed_channel = ingress.fixed_channel.clone();
	let source_label = format!("V2/Flowgraph/Voice/Vosk node={}", node_id);

	let sink = VoiceSink::new(trigger, node_id, fixed_channel);

	let stop_flag = Arc::new(AtomicBool::new(false));
	let stop_flag_worker = stop_flag.clone();
	let join_handle = std::thread::Builder::new()
		.name("vac-voice-vosk-fg".into())
		.spawn(
			move || match run_vosk_loop(tokio_handle, sink, model_dir, source_label.clone(), stop_flag_worker) {
				Ok(()) => {}
				Err(e) => {
					log::error!("《Flowgraph/Voice》: Vosk スレッドが終了しました: {} ({})", e, source_label)
				}
			},
		)
		.ok()?;

	Some(VoiceIngress { join_handle, stop_flag })
}

fn linear_resample_mono(mono: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
	if from_hz == to_hz || mono.is_empty() {
		return mono.to_vec();
	}
	let ratio = from_hz as f64 / to_hz as f64;
	let out_len = ((mono.len() as f64) / ratio).floor() as usize;
	if out_len == 0 {
		return Vec::new();
	}
	let mut out = Vec::with_capacity(out_len);
	for i in 0..out_len {
		let src_f = i as f64 * ratio;
		let i0 = src_f.floor() as usize;
		let frac = src_f - i0 as f64;
		let s0 = mono.get(i0).copied().unwrap_or(0.0);
		let s1 = mono.get(i0 + 1).copied().unwrap_or(s0);
		out.push((s0 as f64 * (1.0 - frac) + s1 as f64 * frac) as f32);
	}
	out
}

fn interleaved_to_mono(data: &[f32], channels: usize) -> Vec<f32> {
	if channels <= 1 {
		return data.to_vec();
	}
	if data.len() < channels {
		return Vec::new();
	}
	let frames = data.len() / channels;
	let mut mono = Vec::with_capacity(frames);
	for f in 0..frames {
		let base = f * channels;
		let mut sum = 0.0_f32;
		for c in 0..channels {
			sum += data[base + c];
		}
		mono.push(sum / channels as f32);
	}
	mono
}

fn f32_mono_to_i16_16k(mono: &[f32]) -> Vec<i16> {
	mono.iter()
		.map(|&s| {
			let v = (s * 32768.0).clamp(-32768.0, 32767.0);
			v as i16
		})
		.collect()
}

fn complete_result_text(r: CompleteResult<'_>) -> String {
	let raw = match r {
		CompleteResult::Single(s) => s.text.trim().to_string(),
		CompleteResult::Multiple(_) => String::new(),
	};
	crate::utility::normalize_interstitial_japanese_spaces(&raw)
}

fn run_vosk_loop(
	rt: tokio::runtime::Handle,
	mut sink: VoiceSink,
	model_dir: String,
	source_label: String,
	stop_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	log::info!("《Voice》: Vosk モデルを解決します spec={} ({})", model_dir, source_label);

	let model_dir = super::voice_vosk_model::resolve_vosk_model_dir(&model_dir)
		.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
	let model_dir_str = model_dir.to_str().ok_or("Vosk モデルパスが UTF-8 ではありません")?.to_string();

	log::info!("《Voice》: Vosk モデルを読み込みます dir={} ({})", model_dir_str, source_label);

	let model = Model::new(model_dir_str.clone()).ok_or_else(|| {
		format!(
			"Vosk Model::new に失敗しました（パス・ファイル破損・モデル不一致の可能性）: {}",
			model_dir_str
		)
	})?;

	let mut recognizer = Recognizer::new(&model, 16000.0).ok_or("Vosk Recognizer::new に失敗しました")?;
	// 部分認識は partial_result() のテキストのみ利用（単語メタデータは不要）
	recognizer.set_partial_words(false);

	let host = cpal::default_host();
	let device = host.default_input_device().ok_or("既定の入力オーディオデバイスがありません")?;
	let supported = device.default_input_config()?;
	let sample_format = supported.sample_format();
	let in_hz_u32 = supported.sample_rate();
	let channels = supported.channels() as usize;
	let stream_config: StreamConfig = supported.into();

	log::info!(
		"《Voice》: 入力 {} Hz, {} ch, format={:?} (Vosk へは 16 kHz にリサンプル)",
		in_hz_u32,
		channels,
		sample_format
	);

	let native_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
	let native_cb = native_buf.clone();
	let err_fn = |e| log::error!("《Voice》: cpal ストリームエラー: {}", e);

	let stream: Stream = match sample_format {
		SampleFormat::F32 => device.build_input_stream(
			&stream_config,
			move |data: &[f32], _| {
				let mono = interleaved_to_mono(data, channels);
				if let Ok(mut g) = native_cb.lock() {
					g.extend_from_slice(&mono);
					trim_native_buf(&mut g, in_hz_u32);
				}
			},
			err_fn,
			None,
		)?,
		SampleFormat::I16 => device.build_input_stream(
			&stream_config,
			move |data: &[i16], _| {
				let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
				let mono = interleaved_to_mono(&f, channels);
				if let Ok(mut g) = native_cb.lock() {
					g.extend_from_slice(&mono);
					trim_native_buf(&mut g, in_hz_u32);
				}
			},
			err_fn,
			None,
		)?,
		SampleFormat::U16 => device.build_input_stream(
			&stream_config,
			move |data: &[u16], _| {
				let f: Vec<f32> = data.iter().map(|&s| ((s as i32) - 32768) as f32 / 32768.0).collect();
				let mono = interleaved_to_mono(&f, channels);
				if let Ok(mut g) = native_cb.lock() {
					g.extend_from_slice(&mono);
					trim_native_buf(&mut g, in_hz_u32);
				}
			},
			err_fn,
			None,
		)?,
		f => {
			return Err(format!("未対応のサンプル形式: {:?}", f).into());
		}
	};

	stream.play()?;
	let _keep_stream_alive = stream;

	/// 一度に処理するネイティブサンプル数（おおよそ 80ms @48kHz 相当の上限）
	const MAX_PULL_NATIVE: usize = 4096;
	/// Vosk へ送る i16 チャンク長（約 200ms @16kHz）
	const VOSK_CHUNK: usize = 3200;

	let mut pending_i16: Vec<i16> = Vec::with_capacity(VOSK_CHUNK * 2);
	let mut last_partial_sent: String = String::new();

	log::info!("《Voice》: Vosk 録音・認識ループを開始しました ({})", source_label);

	loop {
		if stop_flag.load(Ordering::Relaxed) {
			log::info!("《Voice》: Vosk ループに shutdown 要求を受信したため終了します ({})", source_label);
			break Ok(());
		}
		std::thread::sleep(Duration::from_millis(40));

		let pulled: Vec<f32> = {
			let mut guard = match native_buf.lock() {
				Ok(g) => g,
				Err(e) => {
					log::warn!("《Voice》: バッファロック失敗: {}", e);
					continue;
				}
			};
			let take = guard.len().min(MAX_PULL_NATIVE);
			if take == 0 {
				continue;
			}
			guard.drain(0..take).collect()
		};

		let mono_16k = linear_resample_mono(&pulled, in_hz_u32, 16000);
		pending_i16.extend(f32_mono_to_i16_16k(&mono_16k));

		while pending_i16.len() >= VOSK_CHUNK {
			let chunk: Vec<i16> = pending_i16.drain(0..VOSK_CHUNK).collect();
			match recognizer.accept_waveform(&chunk) {
				Ok(DecodingState::Running) => {
					let pr = recognizer.partial_result();
					let text = crate::utility::normalize_interstitial_japanese_spaces(pr.partial.trim());
					if text.is_empty() || text == last_partial_sent {
						continue;
					}
					log::trace!("《Voice》: 認識（Vosk・部分） {:?}", text);
					last_partial_sent = text.clone();
					sink.on_partial(&rt, text);
				}
				Ok(DecodingState::Finalized) => {
					let text = complete_result_text(recognizer.result());
					recognizer.reset();
					last_partial_sent.clear();
					if !text.is_empty() {
						log::debug!("《Voice》: 認識（Vosk・確定） {:?}", text);
					}
					sink.on_final(&rt, text);
				}
				Ok(DecodingState::Failed) => {
					log::trace!("《Voice》: Vosk DecodingState::Failed（無音等）");
				}
				Err(e) => log::warn!("《Voice》: Vosk accept_waveform: {:?}", e),
			}
		}
	}
}

fn trim_native_buf(g: &mut Vec<f32>, in_hz: u32) {
	const MAX_NATIVE_SECS: f32 = 30.0;
	let max_samples = (MAX_NATIVE_SECS * in_hz as f32) as usize;
	let len = g.len();
	if len > max_samples {
		g.drain(0..(len - max_samples));
	}
}
