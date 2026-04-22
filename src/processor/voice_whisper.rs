//! 《Voice》: Whisper (whisper.cpp) でマイク入力を文字起こしする。
//!
//! δ-9 Part E.2 以降は `flowgraph.ingress.voice` からのみ起動される（V1 `[[processors]]` 経路は廃止）。
//! チャンク単位の確定テキストを [`crate::processor::voice::VoiceSink`] に流して TriggerEvent を発火させる。

use crate::bridges::voice::FlowgraphVoiceIngress;
use crate::flowgraph::node::TriggerHandle;
use crate::processor::voice::{VoiceIngress, VoiceSink};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use whisper_rs::{
	install_logging_hooks, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
	WhisperState,
};

/// `flowgraph.ingress.voice` (engine=whisper) 経路の Whisper ワーカー起動。
pub(crate) fn spawn_from_flowgraph(
	ingress: &FlowgraphVoiceIngress,
	tokio_handle: tokio::runtime::Handle,
	trigger: TriggerHandle,
) -> Option<VoiceIngress> {
	if ingress.model_path.trim().is_empty() {
		log::error!(
			"《Flowgraph/Voice》: ノード {} の `model_path` が空です。Whisper GGML モデルパスを指定してください。",
			ingress.node_id
		);
		return None;
	}
	let model_spec = ingress.model_path.clone();
	// ingress.sample_rate は Whisper 側では 16kHz 固定内部。チャンク長は既定 5 秒。
	let chunk_secs = 5.0_f32;
	let lang = whisper_lang_primary(&ingress.language_code);
	let node_id = ingress.node_id.clone();
	let fixed_channel = ingress.fixed_channel.clone();
	let source_label = format!("V2/Flowgraph/Voice/Whisper node={}", node_id);

	let sink = VoiceSink::new(trigger, node_id, fixed_channel);

	let stop_flag = Arc::new(AtomicBool::new(false));
	let stop_flag_worker = stop_flag.clone();
	let join_handle = std::thread::Builder::new()
		.name("vac-voice-whisper-fg".into())
		.spawn(move || {
			install_logging_hooks();
			match run_whisper_loop(
				tokio_handle,
				sink,
				model_spec,
				chunk_secs,
				lang,
				source_label.clone(),
				stop_flag_worker,
			) {
				Ok(()) => {},
				Err(e) => log::error!(
					"《Flowgraph/Voice》: Whisper スレッドが終了しました: {} ({})",
					e, source_label
				),
			}
		})
		.ok()?;

	Some(VoiceIngress { join_handle, stop_flag })
}

fn whisper_lang_primary(s: &str) -> String {
 let t = s.trim();
 if t.is_empty() {
  return "ja".to_string();
 }
 t.split(|c| c == '-' || c == '_')
  .next()
  .unwrap_or("ja")
  .to_string()
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

fn run_whisper_loop(
	rt: tokio::runtime::Handle,
	mut sink: VoiceSink,
	model_spec: String,
	chunk_secs: f32,
	lang: String,
	source_label: String,
	stop_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let model_path = super::voice_whisper_model::resolve_whisper_model(&model_spec)
		.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
	let model_path_str = model_path.to_string_lossy().into_owned();

	log::info!(
		"《Voice》: Whisper モデルを読み込みます path={} chunk={}s lang={} ({})",
		model_path_str,
		chunk_secs,
		lang,
		source_label
	);

 let ctx = WhisperContext::new_with_params(&model_path_str, WhisperContextParameters::default())
  .map_err(|e| format!("WhisperContext: {}", e))?;
 let mut whisper: WhisperState = ctx.create_state().map_err(|e| format!("create_state: {}", e))?;

 let host = cpal::default_host();
 let device = host
  .default_input_device()
  .ok_or("既定の入力オーディオデバイスがありません")?;
 let supported = device.default_input_config()?;
 let sample_format = supported.sample_format();
 let in_hz = supported.sample_rate();
 let channels = supported.channels() as usize;
 let stream_config: StreamConfig = supported.into();

 log::info!(
  "《Voice》: 入力 {:} Hz, {} ch, format={:?}",
  in_hz,
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
     const MAX_NATIVE_SECS: f32 = 30.0;
     let max_samples = (MAX_NATIVE_SECS * in_hz as f32) as usize;
     let len = g.len();
     if len > max_samples {
      g.drain(0..(len - max_samples));
     }
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
     const MAX_NATIVE_SECS: f32 = 30.0;
     let max_samples = (MAX_NATIVE_SECS * in_hz as f32) as usize;
     let len = g.len();
     if len > max_samples {
      g.drain(0..(len - max_samples));
     }
    }
   },
   err_fn,
   None,
  )?,
  SampleFormat::U16 => device.build_input_stream(
   &stream_config,
   move |data: &[u16], _| {
    let f: Vec<f32> = data
     .iter()
     .map(|&s| ((s as i32) - 32768) as f32 / 32768.0)
     .collect();
    let mono = interleaved_to_mono(&f, channels);
    if let Ok(mut g) = native_cb.lock() {
     g.extend_from_slice(&mono);
     const MAX_NATIVE_SECS: f32 = 30.0;
     let max_samples = (MAX_NATIVE_SECS * in_hz as f32) as usize;
     let len = g.len();
     if len > max_samples {
      g.drain(0..(len - max_samples));
     }
    }
   },
   err_fn,
   None,
  )?,
  f => {
   return Err(format!("未対応のサンプル形式: {:?}", f).into());
  },
 };

 stream.play()?;

 let n_threads = num_cpus::get().min(8).max(1) as i32;
 let need_native = (chunk_secs * in_hz as f32).ceil() as usize;

 log::info!("《Voice》: 録音・文字起こしループを開始しました ({})", source_label);

 let _keep_stream_alive = stream;

 loop {
  if stop_flag.load(Ordering::Relaxed) {
   log::info!("《Voice》: Whisper ループに shutdown 要求を受信したため終了します ({})", source_label);
   break Ok(());
  }
  std::thread::sleep(Duration::from_millis(200));

  let pcm_16k: Vec<f32> = {
   let mut guard = match native_buf.lock() {
    Ok(g) => g,
    Err(e) => {
     log::warn!("《Voice》: バッファロック失敗: {}", e);
     continue;
    },
   };
   if guard.len() < need_native {
    continue;
   }
   let chunk: Vec<f32> = guard.drain(0..need_native).collect();
   linear_resample_mono(&chunk, in_hz, 16000)
  };

  if pcm_16k.len() < 1600 {
   continue;
  }

  let rms: f32 = (pcm_16k.iter().map(|x| x * x).sum::<f32>() / pcm_16k.len() as f32).sqrt();
  if rms < 0.002_f32 {
   continue;
  }

  // whisper.cpp 既定は best_of=5。1 だと速いが誤認識が増えやすい。
  let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });
  params.set_n_threads(n_threads);
  params.set_language(Some(&lang));
  params.set_translate(false);
  params.set_print_special(false);
  params.set_print_progress(false);
  params.set_print_realtime(false);
  params.set_print_timestamps(false);

  if let Err(e) = whisper.full(params, &pcm_16k) {
   log::warn!("《Voice》: whisper full 失敗: {}", e);
   continue;
  }

  let mut text = String::new();
  for seg in whisper.as_iter() {
   let s = seg.to_string();
   if !s.trim().is_empty() {
    if !text.is_empty() {
     text.push(' ');
    }
    text.push_str(s.trim());
   }
  }
  let text = text.trim().to_string();
  let text = crate::utility::normalize_interstitial_japanese_spaces(&text);
  if text.is_empty() {
   continue;
  }
  log::debug!("《Voice》: 認識（Whisper・確定） {:?}", text);

  // Whisper はストリーミング partial を出さず、チャンク単位で確定のみ発火する。
  sink.on_final(&rt, text);
 }
}
