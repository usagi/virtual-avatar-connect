//! Flowgraph `FlowgraphProgram` を独立 tokio タスクで実行するためのスポナー（δ-9 Part A）。
//!
//! `FlowgraphProgram` は内部の `StatefulNode` が `Box<dyn Any + Send>` を持つため `Sync` ではない。
//! `Arc<RwLock<State>>` の中には置けないので、本モジュールが
//! 「専用ワーカー task に program を move して所有させる」設計を提供する。
//!
//! 呼び出し側（`State::new` / lib.rs 起動フロー）は
//! [`SpawnedRuntime`] のみを保持し、`trigger` クローンで外部ブリッジ
//! （HTTP / Voice / Twitch ingress）から `TriggerEvent` を投げ込み、
//! アプリ終了時に `shutdown_tx.send(())` でワーカーを停止させる。

use crate::flowgraph::activation::TriggerGate;
use crate::flowgraph::engine::{create_trigger_bus, FlowgraphProgram};
use crate::flowgraph::loader::{Diagnostic, LoadedNodeMeta, Severity};
use crate::flowgraph::node::{ExecCtx, PureEvalHost, TriggerHandle};
use crate::state::State;
use crate::SharedAudioSink;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

/// 起動済みワーカーのハンドル。`State` が保持し、shutdown / ブリッジ配線に使う。
pub struct SpawnedRuntime {
	/// 外部ブリッジから `TriggerEvent` を送り込むための送信ハンドル（Clone+Send+Sync）。
	pub trigger: TriggerHandle,
	/// ワーカー停止用 broadcast。`send(())` で全 program 停止（1 program の今は 1 receiver）。
	pub shutdown_tx: broadcast::Sender<()>,
	/// 起動したワーカーの `JoinHandle`。終了時に `await` すれば grace に待てる。
	pub join: JoinHandle<()>,
	/// `root_dir`（情報表示・GUI 用）。
	pub root_dir: PathBuf,
	/// ロード時診断（diagnostics）のスナップショット。
	pub diagnostics: Vec<Diagnostic>,
	/// ノードメタ（`fq_name` → ファイル / feature 等）。GUI と Control API が参照。
	pub node_meta: HashMap<String, LoadedNodeMeta>,
}

impl std::fmt::Debug for SpawnedRuntime {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SpawnedRuntime")
			.field("root_dir", &self.root_dir)
			.field("nodes", &self.node_meta.len())
			.field("diagnostics", &self.diagnostics.len())
			.finish()
	}
}

impl SpawnedRuntime {
	pub fn has_errors(&self) -> bool {
		self.diagnostics.iter().any(|d| d.severity == Severity::Error)
	}

	/// シャットダウンを通知してワーカー終了を待つ。二重送信は無害。
	pub async fn shutdown(self) {
		let _ = self.shutdown_tx.send(());
		if let Err(e) = self.join.await {
			log::warn!("《Flowgraph》 ワーカー join でエラー: {e}");
		}
	}
}

/// program を新しい tokio タスクに move して起動する。
///
/// 呼び出し直後に返る `(trigger, shutdown_tx, join)` のうち `trigger` は clone 可能で、
/// HTTP / voice / twitch 等の外部ブリッジに配ってイベントを投げ込んでもらう。
pub fn spawn_program(
	mut program: FlowgraphProgram,
	state: Weak<RwLock<State>>,
	audio_sink: Option<SharedAudioSink>,
	trigger_gate: Option<Arc<TriggerGate>>,
	pure_host: PureEvalHost,
) -> (TriggerHandle, broadcast::Sender<()>, JoinHandle<()>) {
	program.pure_host = pure_host;
	let (trigger, rx) = create_trigger_bus();
	let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
	let trigger_for_ctx = trigger.clone();
	let join = tokio::spawn(async move {
		let mut ctx = ExecCtx {
			trace: Vec::new(),
			trigger: Some(trigger_for_ctx.clone()),
			trigger_gate: None,
			node_id: String::new(),
			audio_sink,
			state_handle: Some(state),
			effect_mocks: None,
		};
		let shutdown = async move {
			let _ = shutdown_rx.recv().await;
		};
		match program
			.run_forever_with_bus(&mut ctx, trigger_for_ctx, rx, shutdown, trigger_gate)
			.await
		{
			Ok(run) => log::info!(
				"《Flowgraph》 ランタイムワーカー終了 (generation={}, exec_nodes={})",
				run.generation,
				run.exec_count.len()
			),
			Err(e) => log::error!("《Flowgraph》 ランタイムワーカー異常終了: {e:?}"),
		}
	});
	(trigger, shutdown_tx, join)
}
