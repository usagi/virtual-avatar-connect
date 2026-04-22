//! Flowgraph ランタイムの state（δ-6 で導入、δ-9 Part A で拡張）。
//!
//! `conf.flowgraph_dir` が設定されているとき、起動時に [`crate::flowgraph::load_flowgraph_dir`] を呼ぶ。
//! 本モジュールの [`FlowgraphRuntime`] は **診断 + ノードメタ + 実行ハンドル** を `State` に載せるコンテナ。
//!
//! ## δ-9 以降の役割
//!
//! - ロード成功時は [`crate::flowgraph::spawn::spawn_program`] で専用 tokio ワーカーに program を move し、
//!   `TriggerHandle` と `shutdown` broadcast を手元に残す。
//! - `FlowgraphProgram` は `Box<dyn Any + Send>` を内部に持つため `Sync` でなく `SharedState` には
//!   直置きできない。ここでは program 所有権はワーカー task に全渡しし、本構造体には
//!   Clone 可能な `TriggerHandle` だけを保持する。
//! - 外部ブリッジ（HTTP / Voice / Twitch ingress）は `trigger()` の戻りを clone して `TriggerEvent` を投げ込む。
//! - アプリ終了時は `shutdown()` でワーカーを停止。

use crate::flowgraph::loader::{Diagnostic, LoadedNodeMeta, Severity};
use crate::flowgraph::node::TriggerHandle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

/// 起動済み Flowgraph の実行ハンドル。内部タスクが program を move 所有する。
#[derive(Debug)]
pub struct RuntimeHandle {
	pub trigger: TriggerHandle,
	pub shutdown_tx: broadcast::Sender<()>,
	pub join: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl RuntimeHandle {
	/// シャットダウン broadcast を送り、ワーカーの `join` を await する。多重呼び出し安全。
	pub async fn shutdown(&self) {
		let _ = self.shutdown_tx.send(());
		let mut slot = self.join.lock().await;
		if let Some(h) = slot.take() {
			if let Err(e) = h.await {
				log::warn!("《Flowgraph》 ワーカー join でエラー: {e}");
			}
		}
	}
}

/// Flowgraph のロード状態 + 実行ハンドル。
#[derive(Debug)]
pub struct FlowgraphRuntime {
	pub root_dir: PathBuf,
	/// ロードに成功したか（= error 診断が無かったか）。
	pub ok: bool,
	pub diagnostics: Vec<Diagnostic>,
	pub node_meta: HashMap<String, LoadedNodeMeta>,
	/// δ-9 Part A: ワーカーが生きていれば `Some`。ロード失敗 / 空 / spawn 未実行なら `None`。
	/// `Arc` で包むのは `FlowgraphRuntime` を `State` 経由で clone 参照されても 1 ワーカーに集約するため。
	pub handle: Option<Arc<RuntimeHandle>>,
}

impl Clone for FlowgraphRuntime {
	fn clone(&self) -> Self {
		Self {
			root_dir: self.root_dir.clone(),
			ok: self.ok,
			diagnostics: self.diagnostics.clone(),
			node_meta: self.node_meta.clone(),
			handle: self.handle.clone(),
		}
	}
}

/// `SharedState` から共有される Flowgraph ランタイムのハンドル。
pub type SharedFlowgraph = Arc<RwLock<Option<FlowgraphRuntime>>>;

pub fn shared_flowgraph_new() -> SharedFlowgraph {
	Arc::new(RwLock::new(None))
}

impl FlowgraphRuntime {
	/// `root_dir` を [`crate::flowgraph::load_flowgraph_dir`] でロードし、**program は drop 扱い**の
	/// diagnostic-only runtime を返す（テスト・GUI 単独起動用）。通常運用では
	/// [`FlowgraphRuntime::load_and_spawn`] を使って worker を起こすこと。
	pub fn load(root_dir: &std::path::Path) -> Self {
		let (rt, _program) = Self::load_program(root_dir);
		rt
	}

	/// `load` と違い、ロードした program を第 2 戻り値で返す内部ユーティリティ。
	/// `(runtime_meta, Option<program>)` のペア。
	pub fn load_program(
		root_dir: &std::path::Path,
	) -> (Self, Option<crate::flowgraph::FlowgraphProgram>) {
		if !root_dir.exists() {
			return (Self::empty(root_dir.to_path_buf()), None);
		}
		if !root_dir.is_dir() {
			return (
				Self {
					root_dir: root_dir.to_path_buf(),
					ok: false,
					diagnostics: vec![Diagnostic::error(
						crate::flowgraph::DiagnosticCode::Io,
						format!("flowgraph_dir '{}' はディレクトリではありません", root_dir.display()),
					)],
					node_meta: HashMap::new(),
					handle: None,
				},
				None,
			);
		}
		match crate::flowgraph::load_flowgraph_dir(root_dir) {
			Ok(report) => {
				let crate::flowgraph::LoadReport { program, diagnostics, node_meta } = report;
				let has_nodes = !node_meta.is_empty();
				let rt = Self {
					root_dir: root_dir.to_path_buf(),
					ok: true,
					diagnostics,
					node_meta,
					handle: None,
				};
				(rt, if has_nodes { Some(program) } else { None })
			}
			Err(crate::flowgraph::LoadError { diagnostics }) => (
				Self {
					root_dir: root_dir.to_path_buf(),
					ok: false,
					diagnostics,
					node_meta: HashMap::new(),
					handle: None,
				},
				None,
			),
		}
	}

	/// ロードしてワーカーを spawn する。program がロードできたときのみ `handle` が `Some` になる。
	///
	/// `state_weak` は `ExecCtx.state_handle` に入って `channel.emit` などから
	/// `State::push_channel_datum` を呼ぶ経路。`audio_sink` は TTS 系ノード用。
	pub fn load_and_spawn(
		root_dir: &std::path::Path,
		state_weak: std::sync::Weak<RwLock<crate::state::State>>,
		audio_sink: Option<crate::SharedAudioSink>,
	) -> Self {
		let (mut rt, program) = Self::load_program(root_dir);
		if let Some(program) = program {
			let (trigger, shutdown_tx, join) =
				crate::flowgraph::spawn::spawn_program(program, state_weak, audio_sink);
			rt.handle = Some(Arc::new(RuntimeHandle {
				trigger,
				shutdown_tx,
				join: tokio::sync::Mutex::new(Some(join)),
			}));
		}
		rt
	}

	pub fn empty(root_dir: PathBuf) -> Self {
		Self {
			root_dir,
			ok: true,
			diagnostics: Vec::new(),
			node_meta: HashMap::new(),
			handle: None,
		}
	}

	pub fn has_errors(&self) -> bool {
		self.diagnostics.iter().any(|d| d.severity == Severity::Error)
	}

	pub fn has_warnings(&self) -> bool {
		self.diagnostics.iter().any(|d| d.severity == Severity::Warning)
	}

	/// `trigger()` は worker が起きていれば clone を返す。
	/// 外部ブリッジ（HTTP / Voice / Twitch ingress）が `TriggerEvent` を送る経路。
	pub fn trigger(&self) -> Option<TriggerHandle> {
		self.handle.as_ref().map(|h| h.trigger.clone())
	}
}

/// 起動時のロード結果をログに 1 行ずつ出す。
pub fn log_load_outcome(rt: &FlowgraphRuntime) {
	let errs = rt.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
	let warns = rt.diagnostics.iter().filter(|d| d.severity == Severity::Warning).count();
	let root = rt.root_dir.display();
	if rt.ok && !rt.node_meta.is_empty() {
		log::info!(
			"《Flowgraph》 {} をロードしました: ノード {} 件 / error {} / warning {}。",
			root,
			rt.node_meta.len(),
			errs,
			warns
		);
	} else if rt.ok && rt.diagnostics.is_empty() {
		log::info!("《Flowgraph》 {} は未使用です（ディレクトリ非存在 / 空）。", root);
	} else {
		log::warn!("《Flowgraph》 {} のロードに失敗: error {} / warning {}。", root, errs, warns);
	}
	for d in &rt.diagnostics {
		let label = match d.severity {
			Severity::Error => "error",
			Severity::Warning => "warn",
			Severity::Info => "info",
		};
		let file = d.file.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
		let node = d.node.as_deref().unwrap_or("");
		log::log!(
			match d.severity {
				Severity::Error => log::Level::Error,
				Severity::Warning => log::Level::Warn,
				Severity::Info => log::Level::Info,
			},
			"《Flowgraph》  [{label}] {:?} {} (file={}, node={})",
			d.code,
			d.message,
			file,
			node,
		);
	}
}
