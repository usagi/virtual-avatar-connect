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

use crate::flowgraph::activation::{mode_group_orphan_diagnostics, TriggerGate};
use crate::flowgraph::loader::{Diagnostic, FlowgraphFileActivationMeta, GraphCapabilitySummary, LoadedNodeMeta, Severity};
use crate::flowgraph::node::PureEvalHost;
use crate::flowgraph::node::TriggerHandle;
use crate::flowgraph::{ProgramStateRestoreReport, ProgramStateSnapshot, ProgramStateSummary, StateSnapshotFileError};
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
	/// LF-2: graph 全体の capability summary。GUI と policy preview 用の read-only metadata。
	pub capability_summary: GraphCapabilitySummary,
	/// LF-7: ロード直後の stateful node summary。worker 実行後の live state ではない。
	pub loaded_state_summary: ProgramStateSummary,
	/// LF-7: ロード直後に snapshot export できる state payload。worker 実行後の live state ではない。
	pub loaded_state_snapshot: ProgramStateSnapshot,
	/// RM-3: 各 flowgraph ファイル fq → mode 用メタ（`GET /flowgraph/diagnostics` 等で参照）。
	pub file_activation: HashMap<String, FlowgraphFileActivationMeta>,
	/// RM-3: exec 抑止ゲート（ワーカーと共有）。未 spawn 時は `None`。
	pub trigger_gate: Option<Arc<TriggerGate>>,
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
			capability_summary: self.capability_summary.clone(),
			loaded_state_summary: self.loaded_state_summary.clone(),
			loaded_state_snapshot: self.loaded_state_snapshot.clone(),
			file_activation: self.file_activation.clone(),
			trigger_gate: self.trigger_gate.clone(),
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

	/// `load` と同じ diagnostic-only runtime を返すが、ロード後 metadata を作る前に
	/// 明示 snapshot を restore する。profile-local persistence / reload restore の接続点。
	pub fn load_with_state_snapshot(root_dir: &std::path::Path, snapshot: &ProgramStateSnapshot) -> Self {
		let (rt, _program, _restore) = Self::load_program_with_state_snapshot(root_dir, snapshot);
		rt
	}

	/// `load_with_state_snapshot` と同じ diagnostic-only runtime を返すが、snapshot は
	/// `ProgramStateSnapshotFile` JSON envelope から読み込む。
	pub fn load_with_state_snapshot_file(root_dir: &std::path::Path, snapshot_path: &std::path::Path) -> Self {
		let (rt, _program, _restore) = Self::load_program_with_state_snapshot_file(root_dir, snapshot_path);
		rt
	}

	/// `load` と違い、ロードした program を第 2 戻り値で返す内部ユーティリティ。
	/// `(runtime_meta, Option<program>)` のペア。
	pub fn load_program(root_dir: &std::path::Path) -> (Self, Option<crate::flowgraph::FlowgraphProgram>) {
		let (rt, program, _restore) = Self::load_program_inner(root_dir, None);
		(rt, program)
	}

	/// `load_program` と同じくロードした program も返すが、metadata 生成前に snapshot を restore する。
	/// restore に失敗した場合は error diagnostic を持つ runtime と `None` program を返す。
	pub fn load_program_with_state_snapshot(
		root_dir: &std::path::Path,
		snapshot: &ProgramStateSnapshot,
	) -> (Self, Option<crate::flowgraph::FlowgraphProgram>, Option<ProgramStateRestoreReport>) {
		Self::load_program_inner(root_dir, Some(snapshot))
	}

	/// `load_program_with_state_snapshot` と同じくロードした program も返すが、snapshot は
	/// `ProgramStateSnapshotFile` JSON envelope から読み込む。
	pub fn load_program_with_state_snapshot_file(
		root_dir: &std::path::Path,
		snapshot_path: &std::path::Path,
	) -> (Self, Option<crate::flowgraph::FlowgraphProgram>, Option<ProgramStateRestoreReport>) {
		match crate::flowgraph::read_state_snapshot_file(snapshot_path) {
			Ok(file) => Self::load_program_with_state_snapshot(root_dir, &file.snapshot),
			Err(error) => (
				Self::state_snapshot_file_error(root_dir.to_path_buf(), snapshot_path, error),
				None,
				None,
			),
		}
	}

	fn state_snapshot_file_error(root_dir: PathBuf, snapshot_path: &std::path::Path, error: StateSnapshotFileError) -> Self {
		Self {
			root_dir,
			ok: false,
			diagnostics: vec![Diagnostic::error(
				crate::flowgraph::DiagnosticCode::StateRestore,
				format!("state snapshot file restore failed ({}): {error}", snapshot_path.display()),
			)],
			node_meta: HashMap::new(),
			capability_summary: GraphCapabilitySummary::default(),
			loaded_state_summary: ProgramStateSummary::default(),
			loaded_state_snapshot: ProgramStateSnapshot::default(),
			file_activation: HashMap::new(),
			trigger_gate: None,
			handle: None,
		}
	}

	fn load_program_inner(
		root_dir: &std::path::Path,
		initial_state_snapshot: Option<&ProgramStateSnapshot>,
	) -> (Self, Option<crate::flowgraph::FlowgraphProgram>, Option<ProgramStateRestoreReport>) {
		if !root_dir.exists() {
			return (Self::empty(root_dir.to_path_buf()), None, None);
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
					capability_summary: GraphCapabilitySummary::default(),
					loaded_state_summary: ProgramStateSummary::default(),
					loaded_state_snapshot: ProgramStateSnapshot::default(),
					file_activation: HashMap::new(),
					trigger_gate: None,
					handle: None,
				},
				None,
				None,
			);
		}
		match crate::flowgraph::load_flowgraph_dir(root_dir) {
			Ok(report) => {
				let crate::flowgraph::LoadReport {
					mut program,
					mut diagnostics,
					node_meta,
					capability_summary,
					file_activation,
					..
				} = report;
				let state_restore = if let Some(snapshot) = initial_state_snapshot {
					match program.restore_state_snapshot(snapshot) {
						Ok(report) => Some(report),
						Err(error) => {
							diagnostics.push(Diagnostic::error(
								crate::flowgraph::DiagnosticCode::StateRestore,
								format!("state snapshot restore failed: {error}"),
							));
							let rt = Self {
								root_dir: root_dir.to_path_buf(),
								ok: false,
								diagnostics,
								node_meta,
								capability_summary,
								loaded_state_summary: program.state_summary(),
								loaded_state_snapshot: program.export_state_snapshot(),
								file_activation,
								trigger_gate: None,
								handle: None,
							};
							return (rt, None, None);
						}
					}
				} else {
					None
				};
				let loaded_state_summary = program.state_summary();
				let loaded_state_snapshot = program.export_state_snapshot();
				let has_nodes = !node_meta.is_empty();
				let rt = Self {
					root_dir: root_dir.to_path_buf(),
					ok: true,
					diagnostics,
					node_meta,
					capability_summary,
					loaded_state_summary,
					loaded_state_snapshot,
					file_activation,
					trigger_gate: None,
					handle: None,
				};
				(rt, if has_nodes { Some(program) } else { None }, state_restore)
			}
			Err(crate::flowgraph::LoadError { diagnostics }) => (
				Self {
					root_dir: root_dir.to_path_buf(),
					ok: false,
					diagnostics,
					node_meta: HashMap::new(),
					capability_summary: GraphCapabilitySummary::default(),
					loaded_state_summary: ProgramStateSummary::default(),
					loaded_state_snapshot: ProgramStateSnapshot::default(),
					file_activation: HashMap::new(),
					trigger_gate: None,
					handle: None,
				},
				None,
				None,
			),
		}
	}

	/// ロードしてワーカーを spawn する。program がロードできたときのみ `handle` が `Some` になる。
	///
	/// `state_weak` は `ExecCtx.state_handle` に入って `channel.emit` などから
	/// `State::push_channel_datum` を呼ぶ経路。`audio_sink` は TTS 系ノード用。
	///
	/// `conf`: RM-3 の exec 抑止に使う。`None` のときは全ノード exec 許可（互換）。
	///
	/// `runtime_mode`: 実行中の現在 mode ID。`None` は `conf.default_runtime_mode` に従う（conf も無ければ従来ロジック）。
	/// `runtime_mode_id`: `State` と共有する mode 上書きスロット。Pure ノード `flowgraph.mode.*` が参照する。
	pub fn load_and_spawn(
		root_dir: &std::path::Path,
		state_weak: std::sync::Weak<RwLock<crate::state::State>>,
		audio_sink: Option<crate::SharedAudioSink>,
		conf: Option<&crate::conf::Conf>,
		runtime_mode: Option<&str>,
		runtime_mode_id: Option<std::sync::Arc<std::sync::RwLock<Option<String>>>>,
	) -> Self {
		let (mut rt, program) = Self::load_program(root_dir);
		if let Some(c) = conf {
			if !rt.has_errors() {
				rt.diagnostics.extend(mode_group_orphan_diagnostics(c, &rt.file_activation));
			}
		}
		if let Some(program) = program {
			let gate = Some(if let Some(c) = conf {
				TriggerGate::new(c, &rt.node_meta, &rt.file_activation, runtime_mode)
			} else {
				TriggerGate::all_exec_active()
			});
			let pure_host = PureEvalHost {
				runtime_mode: runtime_mode_id,
				default_runtime_mode: conf.and_then(|c| c.default_runtime_mode.clone()),
			};
			let (trigger, shutdown_tx, join) =
				crate::flowgraph::spawn::spawn_program(program, state_weak, audio_sink, gate.clone(), pure_host);
			rt.trigger_gate = gate;
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
			capability_summary: GraphCapabilitySummary::default(),
			loaded_state_summary: ProgramStateSummary::default(),
			loaded_state_snapshot: ProgramStateSnapshot::default(),
			file_activation: HashMap::new(),
			trigger_gate: None,
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

	/// RM-3: `runtime_mode` を反映して exec ゲートを再計算。ワーカーがいなければ何もしない。
	pub fn recompute_trigger_gate(&self, conf: &crate::conf::Conf, runtime_mode: Option<&str>) -> bool {
		if let Some(g) = self.trigger_gate.as_ref() {
			g.recompute(conf, runtime_mode, &self.node_meta, &self.file_activation);
			return true;
		}
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::{ProgramStateSnapshotFile, ProgramStateSnapshotNode, StateSnapshotFormat};

	fn state_counter_root() -> std::path::PathBuf {
		std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("flowgraph.example")
			.join("state-counter")
	}

	fn counter_snapshot(node: &str, feature: &str, version: u64, value: i64) -> ProgramStateSnapshot {
		ProgramStateSnapshot {
			snapshot_node_count: 1,
			nodes: vec![ProgramStateSnapshotNode {
				node: node.into(),
				feature: feature.into(),
				version,
				format: StateSnapshotFormat::Json,
				value: serde_json::json!({ "value": value }),
			}],
		}
	}

	fn temp_snapshot_path(name: &str) -> std::path::PathBuf {
		let dir = std::env::temp_dir().join(format!("vac-runtime-state-snapshot-{}", std::process::id()));
		std::fs::create_dir_all(&dir).expect("create temp dir");
		dir.join(name)
	}

	#[test]
	fn load_reports_loaded_state_summary_and_snapshot() {
		let root = state_counter_root();
		let rt = FlowgraphRuntime::load(&root);

		assert!(rt.ok, "diagnostics: {:#?}", rt.diagnostics);
		assert_eq!(rt.loaded_state_summary.stateful_node_count, 1);
		assert_eq!(rt.loaded_state_summary.snapshot_supported_node_count, 1);
		assert_eq!(rt.loaded_state_summary.restore_supported_node_count, 1);
		assert_eq!(rt.loaded_state_summary.nodes[0].node, "main::counter");
		assert_eq!(rt.loaded_state_summary.nodes[0].version, 0);
		assert_eq!(rt.loaded_state_snapshot.snapshot_node_count, 1);
		assert_eq!(rt.loaded_state_snapshot.nodes[0].node, "main::counter");
		assert_eq!(rt.loaded_state_snapshot.nodes[0].version, 0);
		assert_eq!(rt.loaded_state_snapshot.nodes[0].value, serde_json::json!({ "value": 0 }));
	}

	#[test]
	fn load_program_with_state_snapshot_restores_before_metadata() {
		let root = state_counter_root();
		let snapshot = counter_snapshot("main::counter", "flowgraph.state.int_counter", 41, 41);
		let (rt, program, restore) = FlowgraphRuntime::load_program_with_state_snapshot(&root, &snapshot);

		assert!(rt.ok, "diagnostics: {:#?}", rt.diagnostics);
		let restore = restore.expect("restore report");
		assert_eq!(restore.restored_node_count, 1);
		assert_eq!(restore.nodes[0].node, "main::counter");
		assert_eq!(restore.nodes[0].version, 41);
		assert!(program.is_some());
		assert_eq!(rt.loaded_state_summary.nodes[0].node, "main::counter");
		assert_eq!(rt.loaded_state_summary.nodes[0].version, 41);
		assert_eq!(rt.loaded_state_snapshot.snapshot_node_count, 1);
		assert_eq!(rt.loaded_state_snapshot.nodes[0].node, "main::counter");
		assert_eq!(rt.loaded_state_snapshot.nodes[0].version, 41);
		assert_eq!(rt.loaded_state_snapshot.nodes[0].value, serde_json::json!({ "value": 41 }));
	}

	#[test]
	fn load_program_with_state_snapshot_reports_restore_error() {
		let root = state_counter_root();
		let snapshot = counter_snapshot("main::counter", "flowgraph.state.bool", 1, 1);
		let (rt, program, restore) = FlowgraphRuntime::load_program_with_state_snapshot(&root, &snapshot);

		assert!(!rt.ok);
		assert!(program.is_none());
		assert!(restore.is_none());
		assert!(rt.diagnostics.iter().any(|diagnostic| {
			diagnostic.code == crate::flowgraph::DiagnosticCode::StateRestore && diagnostic.message.contains("feature mismatch")
		}));
	}

	#[test]
	fn load_program_with_state_snapshot_file_restores_before_metadata() {
		let root = state_counter_root();
		let snapshot_path = temp_snapshot_path("restore-ok.snapshot.json");
		let snapshot = counter_snapshot("main::counter", "flowgraph.state.int_counter", 42, 42);
		let file = ProgramStateSnapshotFile::with_created_at_unix_ms(snapshot, 1234);
		crate::flowgraph::write_state_snapshot_file(&snapshot_path, &file).expect("write snapshot file");

		let (rt, program, restore) = FlowgraphRuntime::load_program_with_state_snapshot_file(&root, &snapshot_path);

		assert!(rt.ok, "diagnostics: {:#?}", rt.diagnostics);
		assert!(program.is_some());
		assert_eq!(restore.expect("restore report").restored_node_count, 1);
		assert_eq!(rt.loaded_state_snapshot.nodes[0].version, 42);
		assert_eq!(rt.loaded_state_snapshot.nodes[0].value, serde_json::json!({ "value": 42 }));
		let _ = std::fs::remove_file(snapshot_path);
	}

	#[test]
	fn load_program_with_state_snapshot_file_reports_file_error() {
		let root = state_counter_root();
		let snapshot_path = temp_snapshot_path("restore-bad.snapshot.json");
		std::fs::write(&snapshot_path, "{ not json").expect("write bad snapshot file");

		let (rt, program, restore) = FlowgraphRuntime::load_program_with_state_snapshot_file(&root, &snapshot_path);

		assert!(!rt.ok);
		assert!(program.is_none());
		assert!(restore.is_none());
		assert!(rt.diagnostics.iter().any(|diagnostic| {
			diagnostic.code == crate::flowgraph::DiagnosticCode::StateRestore
				&& diagnostic.message.contains("state snapshot file restore failed")
		}));
		let _ = std::fs::remove_file(snapshot_path);
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
