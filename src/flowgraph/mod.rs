//! VAC Flowgraph ランタイム（Phase δ）。
//!
//! 仕様は `docs/roadmap/phase-delta-spec.md` 参照。δ-1 v2 はハイブリッド骨格:
//! - [`socket`]: `SocketType` / `SocketValue`
//! - [`node`]: `NodeSpec` + `PureNode` / `StatefulNode` / `EffectfulNode` trait + `NodeImpl` enum
//! - [`engine`]: `FlowgraphBuilder` / `FlowgraphProgram` + pull-demand lazy + generation
//! - [`nodes`]: Literal / Log / Branch / Sequence（動作確認用）
//! - [`registry`] (δ-5): 組込みノードカタログ（feature → `NodeSpec` / `NodeImpl`）。
//! - [`loader`] (δ-5): `*.flowgraph.toml` ファイル群のパース + fq name 解決 +
//!   `FlowgraphProgram` 構築。単一ファイル / ディレクトリ両対応。
//! - [`runtime`] (δ-6): ロード結果を `State` から共有するためのハンドル（`FlowgraphRuntime`）。

pub mod config;
pub mod docs;
pub mod engine;
pub mod fixture_runner;
pub mod fragment;
pub mod loader;
pub mod osc;
pub mod vmc;
pub mod vrchat;
pub mod activation;
pub mod node;
pub mod nodes;
pub mod quantity;
pub mod registry;
pub mod runtime;
pub mod socket;
pub mod spawn;
pub mod state_model;
pub mod table;
pub mod tts;

pub use config::{parse_offset_str, ConfigError as FlowgraphConfigError, FlowgraphInstanceConfig};
pub use engine::{
	BuildError, Edge, FlowgraphBuilder, FlowgraphProgram, NodeId, NodeInstance, PortName, PortRef, ProgramRun, ProgramStateNode,
	ProgramStateRestoreNode, ProgramStateRestoreReport, ProgramStateSnapshot, ProgramStateSnapshotNode, ProgramStateSummary, StateRestoreError,
};
pub use activation::{
	build_node_exec_active_map, file_effective_exec_active, file_fq_for_node_id, mode_group_orphan_diagnostics, TriggerGate,
};
pub use loader::{
	file_activation_meta, load_file, load_flowgraph_dir, Diagnostic, DiagnosticCode, FlowgraphFileActivationMeta, LoadError,
	LoadReport, LoadedNodeMeta, Severity,
};
pub use node::{
	EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeImpl, NodeOutput, NodeSpec, OutputMap, PortDirection,
	PortSpec, PropertySpec, PureEvalHost, PureNode, SocketValueRepr, StatefulNode,
};
pub use registry::{default_registry, registry, NodeRegistry};
pub use runtime::{shared_flowgraph_new, FlowgraphRuntime, RuntimeHandle, SharedFlowgraph};
pub use socket::{SocketType, SocketValue, TypeParseError, ValueCastError};
pub use state_model::{
	FlowgraphStateModel, StateLifetime, StateMigrationPolicy, StatePersistencePolicy, StateRestorePolicy, StateScope, StateSnapshotFormat,
	StateSnapshotPolicy, StateStorage,
};
pub use table::{ColumnSpec, Row, Table, TableFromJsonError, TableSchema};
