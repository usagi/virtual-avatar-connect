//! Loader 診断情報（spec §8.3）。
//!
//! GUI / Control API 向けに JSON serializable。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use thiserror::Error;

use crate::flowgraph::state_model::{
	FlowgraphStateModel, StateLifetime, StateMigrationPolicy, StatePersistencePolicy, StateRestorePolicy, StateScope, StateSnapshotFormat,
	StateSnapshotPolicy, StateStorage,
};
use crate::flowgraph::SocketTypeExpr;

/// 診断の深刻度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
	Error,
	Warning,
	Info,
}

/// 機械可読な診断コード。GUI の "Fix it" ボタン実装で条件分岐に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticCode {
	/// TOML パースエラー。
	TomlParse,
	/// `feature` が `NodeRegistry` に無い。
	UnknownFeature,
	/// `id` 重複（ファイル内 or 全体）。
	DuplicateNodeId,
	/// edge の `from` / `to` が `node:port` 文法に合致しない。
	InvalidPortRef,
	/// edge が参照するノードが見つからない。
	UnresolvedNodeRef,
	/// edge が参照するポートが NodeSpec に存在しない。
	UnknownPort,
	/// プロパティ型がスキーマと一致しない。
	PropertyTypeMismatch,
	/// 必須プロパティが未指定。
	MissingRequiredProperty,
	/// 未知プロパティ（typo 警告）。
	UnknownProperty,
	/// engine `build()` が返した各種検証エラー。
	EngineBuild,
	/// I/O エラー（ファイル読み込み失敗など）。
	Io,
	/// 曖昧な main 省略参照（`X::id` に対して `X/main.flowgraph.toml` が無い）。
	AmbiguousMainRef,
	/// Phase λ: 閉集合 string ポートへ接続した `flowgraph.literal.string` の値が許容集合外。
	ClosedStringLiteralOutOfEnum,
	/// Phase λ: 同一ファイル内で `[[enums]]` の `id` が重複。
	DuplicateEnumId,
	/// Phase λ: `[[enums]]` エントリが不正（空 id / 空 variants 等）。
	InvalidEnumDefinition,
	/// Phase λ: `library_uses` が参照する fq が walk 集合に存在しない。
	UnknownLibraryRef,
	/// Phase λ: `library_uses` の依存グラフに閉路がある。
	LibraryDependencyCycle,
	/// LF-4: `[package]` manifest が不正（空 id / 不明 export fq 等）。
	InvalidPackageManifest,
	/// LF-4: `flowgraph.lock.json` が stale または不正。
	PackageLockFile,
	/// LF-5: `[[types]]` named schema metadata が不正、または `record<schema_id>` の参照先が未定義。
	InvalidTypeDefinition,
	/// RM-3: `[meta].mode_groups` など activation メタが不正（空のグループ名など）。
	InvalidModeMetadata,
	/// RM-3: `[meta].mode_groups` の名前が、いかなる `[modes].flowgraph_groups` でも使われていない。
	OrphanModeGroup,
	/// LF-7: load-time state snapshot restore failed.
	StateRestore,
}

/// 単一診断メッセージ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
	pub severity: Severity,
	pub code: DiagnosticCode,
	pub message: String,
	/// 発生源ファイル（複数ファイル統合時に特定用）。相対パス推奨。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub file: Option<PathBuf>,
	/// ノード ID（判明している場合）。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub node: Option<String>,
	/// edge インデックスまたは port 名など、文脈ヒント。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub hint: Option<String>,
}

impl Diagnostic {
	pub fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
		Self {
			severity: Severity::Error,
			code,
			message: message.into(),
			file: None,
			node: None,
			hint: None,
		}
	}

	pub fn warning(code: DiagnosticCode, message: impl Into<String>) -> Self {
		Self {
			severity: Severity::Warning,
			code,
			message: message.into(),
			file: None,
			node: None,
			hint: None,
		}
	}

	pub fn with_file(mut self, file: impl Into<PathBuf>) -> Self {
		self.file = Some(file.into());
		self
	}

	pub fn with_node(mut self, node: impl Into<String>) -> Self {
		self.node = Some(node.into());
		self
	}

	pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
		self.hint = Some(hint.into());
		self
	}
}

/// Loader 失敗時のトップレベルエラー。
///
/// 原則として `diagnostics` にまとめて詰めて返す。最初の 1 件でアボートはしない。
#[derive(Debug, Error)]
#[error("flowgraph のロードに失敗 ({} error / {} warning)",
  diagnostics.iter().filter(|d| d.severity == Severity::Error).count(),
  diagnostics.iter().filter(|d| d.severity == Severity::Warning).count())]
pub struct LoadError {
	pub diagnostics: Vec<Diagnostic>,
}

impl LoadError {
	pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
		Self { diagnostics }
	}

	pub fn from_single(d: Diagnostic) -> Self {
		Self { diagnostics: vec![d] }
	}

	pub fn has_errors(&self) -> bool {
		self.diagnostics.iter().any(|d| d.severity == Severity::Error)
	}

	pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
		self.diagnostics.iter().filter(|d| d.severity == Severity::Error)
	}

	pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
		self.diagnostics.iter().filter(|d| d.severity == Severity::Warning)
	}
}

/// RM-3: ファイル fq（例 `tts/main`）→ Mode Manager 向け activation メタ。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowgraphFileActivationMeta {
	#[serde(default)]
	pub mode_groups: Vec<String>,
	#[serde(default = "crate::utility::bool_true")]
	pub default_enabled: bool,
}

impl Default for FlowgraphFileActivationMeta {
	fn default() -> Self {
		Self {
			mode_groups: Vec::new(),
			default_enabled: true,
		}
	}
}

/// 成功ロード結果。warning が付帯しうる。
pub struct LoadReport {
	pub program: crate::flowgraph::FlowgraphProgram,
	pub diagnostics: Vec<Diagnostic>,
	/// 解決済みノード（`fq_name` → 定義元ファイル / feature）。GUI / debug 用メタ情報。
	pub node_meta: std::collections::HashMap<String, LoadedNodeMeta>,
	/// LF-1: graph-as-node / library signature の入口となる read-only graph signature。
	pub graph_signature: GraphSignature,
	/// LF-4: `[package]` manifest を file ごとに集約した read-only package catalog。
	pub package_manifests: Vec<PackageManifestSummary>,
	/// LF-4: local package dependency DAG の dependency-first order。
	pub package_dependency_order: Vec<String>,
	/// LF-4: lockfile 生成前に diagnostics API から観測できる read-only package lock preview。
	pub package_lock_preview: Vec<PackageLockEntry>,
	/// LF-5: `[[types]]` named schema metadata catalog。
	pub type_schemas: Vec<TypeSchemaSummary>,
	/// LF-4: package lock preview 全体の stable digest。空 preview では `None`。
	pub package_lock_preview_digest: Option<String>,
	/// LF-2: graph 全体が要求する capability の集計。policy enforcement ではなく read-only metadata。
	pub capability_summary: GraphCapabilitySummary,
	/// RM-3: 各 `.flowgraph.toml` の fq → `[meta]` の mode 系メタ（省略時は既定）。
	pub file_activation: std::collections::HashMap<String, FlowgraphFileActivationMeta>,
}

impl std::fmt::Debug for LoadReport {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LoadReport")
			.field("nodes", &self.node_meta.keys().collect::<Vec<_>>())
			.field("graph_signature", &self.graph_signature)
			.field("package_manifests", &self.package_manifests)
			.field("package_dependency_order", &self.package_dependency_order)
			.field("package_lock_preview", &self.package_lock_preview)
			.field("type_schemas", &self.type_schemas)
			.field("package_lock_preview_digest", &self.package_lock_preview_digest)
			.field("capability_summary", &self.capability_summary)
			.field("file_activation", &self.file_activation.keys().collect::<Vec<_>>())
			.field("diagnostics", &self.diagnostics)
			.finish()
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageManifestSummary {
	pub source_fq: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub source_digest: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub id: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub version: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub exports: Vec<String>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageLockEntry {
	pub id: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub version: Option<String>,
	pub source_fq: String,
	/// Stable digest of the source `.flowgraph.toml` content, encoded as `b3:<64 hex>`.
	pub source_digest: String,
	/// Stable digest of package lock entry identity fields, encoded as `b3:<64 hex>`.
	pub digest: String,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeSchemaSummary {
	pub source_fq: String,
	pub id: String,
	pub kind: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub fields: BTreeMap<String, String>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub field_type_exprs: BTreeMap<String, SocketTypeExpr>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub field_record_refs: BTreeMap<String, Vec<String>>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub record_refs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphSignature {
	pub version: u32,
	pub kind: String,
	pub file_count: usize,
	pub node_count: usize,
	pub edge_count: usize,
	pub effectful_node_count: usize,
	pub stateful_node_count: usize,
	pub snapshot_supported_state_node_count: usize,
	pub restore_supported_state_node_count: usize,
	pub required_capabilities: Vec<String>,
	pub files: Vec<GraphSignatureFile>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub type_schemas: Vec<TypeSchemaSummary>,
	pub external_triggers: Vec<GraphSignatureTrigger>,
	pub boundary_inputs: Vec<GraphSignaturePort>,
	pub boundary_outputs: Vec<GraphSignaturePort>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphSignatureFile {
	pub fq: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub title: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub library_id: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub package_id: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub package_version: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub package_exports: Vec<String>,
	pub mode_groups: Vec<String>,
	pub default_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphSignaturePort {
	pub node: String,
	pub feature: String,
	pub port: String,
	pub label: String,
	pub ty: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub type_expr: Option<crate::flowgraph::socket::SocketTypeExpr>,
	pub direction: String,
	pub exec: bool,
	pub optional: bool,
	pub multi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphSignatureTrigger {
	pub node: String,
	pub feature: String,
	pub trigger_kind: String,
	pub exec_inputs: Vec<String>,
	pub data_inputs: Vec<GraphSignaturePort>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphCapabilitySummary {
	pub node_count: usize,
	pub effectful_node_count: usize,
	#[serde(default)]
	pub stateful_node_count: usize,
	#[serde(default)]
	pub volatile_state_node_count: usize,
	#[serde(default)]
	pub snapshot_supported_state_node_count: usize,
	#[serde(default)]
	pub restore_supported_state_node_count: usize,
	pub capabilities: Vec<String>,
	pub capability_counts: BTreeMap<String, usize>,
	pub nodes: Vec<GraphCapabilityNode>,
	#[serde(default)]
	pub state_nodes: Vec<GraphStateNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphCapabilityNode {
	pub node: String,
	pub feature: String,
	pub effect_class: String,
	pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphStateNode {
	pub node: String,
	pub feature: String,
	pub scope: StateScope,
	pub storage: StateStorage,
	pub lifetime: StateLifetime,
	pub reinitialized_on_reload: bool,
	pub snapshot_supported: bool,
	pub snapshot_policy: StateSnapshotPolicy,
	pub snapshot_format: StateSnapshotFormat,
	pub restore_supported: bool,
	pub restore_policy: StateRestorePolicy,
	pub migration_policy: StateMigrationPolicy,
	pub persistence_policy: StatePersistencePolicy,
}

impl GraphCapabilitySummary {
	pub fn from_node_meta(
		reg: &crate::flowgraph::registry::NodeRegistry,
		node_meta: &std::collections::HashMap<String, LoadedNodeMeta>,
	) -> Self {
		let mut nodes = Vec::new();
		let mut state_nodes = Vec::new();
		let mut capabilities = BTreeSet::new();
		let mut effectful_node_count = 0;
		let mut stateful_node_count = 0;

		for (node, meta) in node_meta {
			let effect_class = reg.effect_class(&meta.feature).unwrap_or("unknown").to_string();
			if effect_class == "effectful" {
				effectful_node_count += 1;
			}
			if effect_class == "stateful" {
				stateful_node_count += 1;
				let state_model = reg
					.state_model(&meta.feature)
					.unwrap_or_else(|| FlowgraphStateModel::for_effect_class(&effect_class));
				state_nodes.push(GraphStateNode {
					node: node.clone(),
					feature: meta.feature.clone(),
					scope: state_model.scope,
					storage: state_model.storage,
					lifetime: state_model.lifetime,
					reinitialized_on_reload: state_model.reinitialized_on_reload,
					snapshot_supported: state_model.snapshot_supported,
					snapshot_policy: state_model.snapshot_policy,
					persistence_policy: state_model.persistence_policy,
					snapshot_format: state_model.snapshot_format,
					restore_supported: state_model.restore_supported,
					restore_policy: state_model.restore_policy,
					migration_policy: state_model.migration_policy,
				});
			}
			let node_caps: Vec<String> = reg.capabilities(&meta.feature).into_iter().map(str::to_string).collect();
			for cap in &node_caps {
				capabilities.insert(cap.clone());
			}
			if !node_caps.is_empty() || effect_class != "pure" {
				nodes.push(GraphCapabilityNode {
					node: node.clone(),
					feature: meta.feature.clone(),
					effect_class,
					capabilities: node_caps,
				});
			}
		}

		nodes.sort_by(|a, b| a.node.cmp(&b.node));
		state_nodes.sort_by(|a, b| a.node.cmp(&b.node));
		let capability_counts = nodes.iter().fold(BTreeMap::new(), |mut out, node| {
			for cap in &node.capabilities {
				*out.entry(cap.clone()).or_insert(0) += 1;
			}
			out
		});
		Self {
			node_count: node_meta.len(),
			effectful_node_count,
			stateful_node_count,
			volatile_state_node_count: state_nodes.iter().filter(|node| node.storage == StateStorage::Volatile).count(),
			snapshot_supported_state_node_count: state_nodes.iter().filter(|node| node.snapshot_supported).count(),
			restore_supported_state_node_count: state_nodes.iter().filter(|node| node.restore_supported).count(),
			capabilities: capabilities.into_iter().collect(),
			capability_counts,
			nodes,
			state_nodes,
		}
	}

	pub fn counts_by_capability(&self) -> BTreeMap<&str, usize> {
		let mut out = BTreeMap::new();
		for node in &self.nodes {
			for cap in &node.capabilities {
				*out.entry(cap.as_str()).or_insert(0) += 1;
			}
		}
		out
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedNodeMeta {
	pub feature: String,
	pub file: PathBuf,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub position: Option<[f64; 2]>,
	/// δ-9: 解決済み properties（`PropertySpec` ベースの型変換済）。ブリッジ層が
	/// `path` / `engine` などを読むのに使う。`Serialize` には不要なので skip。
	#[serde(default, skip_serializing, skip_deserializing)]
	pub properties: crate::flowgraph::node::InputMap,
}
