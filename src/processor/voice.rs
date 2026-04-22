//! 《Voice》: 既定の録音デバイスから PCM を取り込み、**Vosk** または **Whisper（whisper.cpp）**で文字起こしし、
//! `flowgraph.ingress.voice` ノードに `TriggerEvent` として流し込む ingress。
//!
//! **既定エンジン**: `vosk`（`engine` プロパティ省略時）。
//!
//! **Vosk**: `model_path`（展開済みディレクトリ、または alphacephei のモデル ID で初回自動取得）。
//!
//! **Whisper**（オプション）: `cargo build --features voice-whisper`（CMake・LLVM・C++）。
//! `engine = "whisper"` と `model_path`（既定ビルドは Vosk のみ）。
//!
//! **入力**: cpal の既定入力デバイス。
//!
//! ## 経路 (δ-9 Part E.2 以降)
//!
//! V1 `[[processors]]` 経路は v1.0 で完全除去済み。本モジュールは `flowgraph.ingress.voice`
//! から [`crate::bridges::voice::spawn`] 経由で呼ばれ、認識結果は [`VoiceSink`] を介して
//! `TriggerHandle::send` で Flowgraph に投入される。
//!
//! MVP では確定テキストのみ流しており、Vosk の partial はログにだけ出して Flowgraph には送らない。
//! 将来 partial 対応する場合は [`VoiceSink::on_partial`] を拡張する。

pub(crate) struct VoiceIngress {
	pub(crate) join_handle: std::thread::JoinHandle<()>,
	/// Worker ループに停止要求を伝えるフラグ。`finish` で立てる。
	pub(crate) stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl VoiceIngress {
	pub(crate) async fn finish(self) {
		use std::sync::atomic::Ordering;
		// 1. stop_flag を立てて worker ループが次のチェックで抜けるよう促す。
		self.stop_flag.store(true, Ordering::Relaxed);
		// 2. 実際のスレッド join は最大 3 秒だけ待つ。CPAL / Vosk / Whisper の
		//    リソース解放に時間がかかるケースもあるが、shutdown 時は OS が
		//    プロセス終了時にクリーンアップするのでここで無限待ちは避ける。
		let join = tokio::task::spawn_blocking(move || {
			let _ = self.join_handle.join();
		});
		if tokio::time::timeout(std::time::Duration::from_secs(3), join).await.is_err() {
			log::warn!(
				"《Flowgraph/Voice》 worker スレッドの join が 3 秒で完了しませんでした。OS に任せて続行します。"
			);
		}
	}
}

use crate::bridges::voice::FlowgraphVoiceIngress;
use crate::flowgraph::node::{TriggerEvent, TriggerHandle};
use crate::flowgraph::socket::SocketValue;

// ---------------------------------------------------------------------
// VoiceSink: 認識結果を `flowgraph.ingress.voice` ノードに流すための薄いラッパ
// ---------------------------------------------------------------------

/// `flowgraph.ingress.voice` ノードへ TriggerEvent を発火するための sink。
///
/// `voice-vosk` / `voice-whisper` のいずれかでビルドされたときに voice_vosk.rs /
/// voice_whisper.rs から使われる。両方無効な `--no-default-features` ビルドでは未使用になるため
/// `#[allow(dead_code)]` で警告を抑える。
#[allow(dead_code)]
pub(crate) struct VoiceSink {
	pub trigger: TriggerHandle,
	pub node_id: String,
	/// `flowgraph.ingress.voice` の `fixed_channel` property。空なら `"voice"` を使う。
	pub fixed_channel: String,
}

#[allow(dead_code)]
impl VoiceSink {
	pub(crate) fn new(trigger: TriggerHandle, node_id: String, fixed_channel: String) -> Self {
		Self { trigger, node_id, fixed_channel }
	}

	/// Vosk の部分認識テキスト。MVP では Flowgraph には流さず、上位のログ出力のみに任せる。
	pub(crate) fn on_partial(&mut self, _rt: &tokio::runtime::Handle, _text: String) {
		// 将来拡張: partial 用 TriggerEvent を送る場合はここで `.with_override("__is_final__", false)` などを載せる。
	}

	/// 確定テキストを `TriggerHandle::send` で投入する。
	pub(crate) fn on_final(&mut self, _rt: &tokio::runtime::Handle, text: String) {
		if text.is_empty() {
			return;
		}
		let source_kind = if self.fixed_channel.is_empty() {
			"voice".to_string()
		} else {
			self.fixed_channel.clone()
		};
		let event = TriggerEvent::new(&self.node_id)
			.with_exec("__trigger__")
			.with_override("__content__", SocketValue::String(text))
			.with_override("__source_kind__", SocketValue::String(source_kind));
		if let Err(e) = self.trigger.send(event) {
			log::warn!(
				"《Flowgraph/Voice》 trigger 送信失敗 node={}: {:?}",
				self.node_id,
				e
			);
		}
	}
}

// ---------------------------------------------------------------------
// エントリ: flowgraph.ingress.voice から spawn
// ---------------------------------------------------------------------

/// `flowgraph.ingress.voice` ノードごとに Vosk / Whisper ワーカーを起動する。
///
/// 認識結果は `TriggerHandle::send` で該当ノードに流れ、graph の exec_out 下流へ伝播する。
/// 戻り値 `None` は engine / feature ビルド / モデルパス不足などで起動失敗を示す。
#[allow(unused_variables)]
pub(crate) fn spawn_from_flowgraph(
	ingress: &FlowgraphVoiceIngress,
	tokio_handle: tokio::runtime::Handle,
	trigger: TriggerHandle,
) -> Option<VoiceIngress> {
	match ingress.engine.as_str() {
		"vosk" => {
			#[cfg(feature = "voice-vosk")]
			{
				super::voice_vosk::spawn_from_flowgraph(ingress, tokio_handle, trigger)
			}
			#[cfg(not(feature = "voice-vosk"))]
			{
				log::error!(
					"《Flowgraph/Voice》: engine=vosk ですが voice-vosk 機能有効でビルドされていません。`cargo build` 既定 or `--features voice-vosk` でビルドしてください。node={}",
					ingress.node_id
				);
				None
			}
		},
		"whisper" | "whisper-rs" => {
			#[cfg(feature = "voice-whisper")]
			{
				super::voice_whisper::spawn_from_flowgraph(ingress, tokio_handle, trigger)
			}
			#[cfg(not(feature = "voice-whisper"))]
			{
				log::error!(
					"《Flowgraph/Voice》: engine=whisper ですが voice-whisper 機能有効でビルドされていません。`cargo build --features voice-whisper` してください。node={}",
					ingress.node_id
				);
				None
			}
		},
		other => {
			log::error!(
				"《Flowgraph/Voice》: 不明な engine {:?}（vosk または whisper を指定してください）node={}",
				other,
				ingress.node_id
			);
			None
		},
	}
}
