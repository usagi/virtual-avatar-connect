//! `flowgraph.ingress.voice` 用ブリッジ（δ-9 Part E.2）。
//!
//! ## 責務
//!
//! - `flowgraph.ingress.voice` ノードの property から [`FlowgraphVoiceIngress`] を生成する。
//! - [`spawn`] でノード 1 件につき Vosk / Whisper ワーカースレッドを起動する。認識結果は
//!   `TriggerHandle::send` で該当ノードに流れ、graph 下流へ伝播する。
//!
//! ## 実装メモ
//!
//! 実際の Vosk / Whisper 呼び出し本体は V1 ingress と共用しており、
//! `processor::voice::spawn_from_flowgraph`（その下の [`crate::processor::voice::VoiceSink`] 抽象経由）
//! を叩く。Flowgraph 経路では V2 sink が組まれ、`channel_data` には push せず
//! `TriggerEvent` のみを投げる。
//!
//! MVP では部分認識（Vosk の partial）は送らず、確定テキスト（finalize / Whisper チャンク）だけを
//! 投げる。speech_floor と連動した floor held 制御は V1 専用のため、Flowgraph 経路では未接続。

use crate::flowgraph::loader::LoadedNodeMeta;
use crate::flowgraph::node::TriggerHandle;
use crate::processor::voice::VoiceIngress;

#[derive(Debug, Clone)]
pub struct FlowgraphVoiceIngress {
	pub node_id: String,
	pub engine: String,
	pub model_path: String,
	/// マイクデバイス名などの音声ソース指定（現状は既定入力のみ使用／将来の対応用）
	#[allow(dead_code)]
	pub source: String,
	/// サンプリングレート（現状 Vosk=16kHz 固定、Whisper=16kHz 内部リサンプル。将来の上書き用）
	#[allow(dead_code)]
	pub sample_rate: i64,
	pub language_code: String,
	/// Vosk の constrained grammar (JSON) — 将来の拡張用で現状は未使用
	#[allow(dead_code)]
	pub grammar_json: String,
	pub fixed_channel: String,
}

impl FlowgraphVoiceIngress {
	pub fn from_meta(fq: &str, meta: &LoadedNodeMeta) -> Option<Self> {
		let p = &meta.properties;
		let get_str = |k: &str| -> String {
			p.get(k)
				.and_then(|v| v.as_str().ok())
				.map(str::trim)
				.map(str::to_string)
				.unwrap_or_default()
		};
		let get_int = |k: &str| -> i64 { p.get(k).and_then(|v| v.as_i64().ok()).unwrap_or(0) };
		Some(Self {
			node_id: fq.to_string(),
			engine: {
				let e = get_str("engine");
				if e.is_empty() {
					"vosk".into()
				} else {
					e.to_ascii_lowercase()
				}
			},
			model_path: get_str("model_path"),
			source: get_str("source"),
			sample_rate: {
				let v = get_int("sample_rate");
				if v <= 0 {
					16000
				} else {
					v
				}
			},
			language_code: {
				let l = get_str("language_code");
				if l.is_empty() {
					"ja".into()
				} else {
					l
				}
			},
			grammar_json: get_str("grammar_json"),
			fixed_channel: get_str("fixed_channel"),
		})
	}
}

/// `flowgraph.ingress.voice` の全エントリに対してワーカースレッドを起動する。
///
/// `trigger` が None の場合（Flowgraph 未起動）は warn ログのみ出して何も spawn しない。
///
/// 戻り値は起動に成功した `VoiceIngress` ハンドル一覧。shutdown 時に `.finish().await` すること。
pub fn spawn(entries: &[FlowgraphVoiceIngress], trigger: Option<TriggerHandle>, tokio_handle: tokio::runtime::Handle) -> Vec<VoiceIngress> {
	if entries.is_empty() {
		return Vec::new();
	}
	let Some(trigger) = trigger else {
		log::warn!(
			"《Flowgraph/Voice》 ingress ノード {} 件を検出しましたが、Flowgraph ランタイムが起動していないため voice bridge を開始できません。",
			entries.len()
		);
		return Vec::new();
	};

	let mut handles = Vec::new();
	for e in entries {
		log::info!(
			"《Flowgraph/Voice》 起動準備 node={} engine={} model_path={:?} source={:?} sample_rate={} lang={} fixed_channel={:?}",
			e.node_id,
			e.engine,
			e.model_path,
			e.source,
			e.sample_rate,
			e.language_code,
			e.fixed_channel
		);
		match crate::processor::voice::spawn_from_flowgraph(e, tokio_handle.clone(), trigger.clone()) {
			Some(h) => {
				log::info!("《Flowgraph/Voice》 ワーカー起動成功 node={} engine={}", e.node_id, e.engine);
				handles.push(h);
			}
			None => {
				log::error!(
					"《Flowgraph/Voice》 ワーカー起動失敗 node={} engine={}（model_path / feature ビルドを確認してください）",
					e.node_id,
					e.engine
				);
			}
		}
	}
	handles
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::InputMap;
	use crate::flowgraph::socket::SocketValue;
	use std::path::PathBuf;

	#[test]
	fn from_meta_reads_all_fields() {
		let mut props = InputMap::new();
		props.insert("engine".into(), SocketValue::String("whisper".into()));
		props.insert("model_path".into(), SocketValue::String("/m".into()));
		props.insert("source".into(), SocketValue::String("mic1".into()));
		props.insert("sample_rate".into(), SocketValue::Int(48000));
		props.insert("language_code".into(), SocketValue::String("en".into()));
		props.insert("fixed_channel".into(), SocketValue::String("vc".into()));
		let meta = LoadedNodeMeta {
			feature: "flowgraph.ingress.voice".into(),
			file: PathBuf::from("x.toml"),
			position: None,
			properties: props,
		};
		let v = FlowgraphVoiceIngress::from_meta("ingress::voice", &meta).unwrap();
		assert_eq!(v.engine, "whisper");
		assert_eq!(v.model_path, "/m");
		assert_eq!(v.source, "mic1");
		assert_eq!(v.sample_rate, 48000);
		assert_eq!(v.language_code, "en");
		assert_eq!(v.fixed_channel, "vc");
	}

	#[test]
	fn defaults_for_empty_properties() {
		let meta = LoadedNodeMeta {
			feature: "flowgraph.ingress.voice".into(),
			file: PathBuf::from("x.toml"),
			position: None,
			properties: InputMap::new(),
		};
		let v = FlowgraphVoiceIngress::from_meta("in", &meta).unwrap();
		assert_eq!(v.engine, "vosk");
		assert_eq!(v.sample_rate, 16000);
		assert_eq!(v.language_code, "ja");
	}
}
