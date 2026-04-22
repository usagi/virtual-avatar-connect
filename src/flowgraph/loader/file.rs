//! 単一 `*.flowgraph.toml` ファイルの schema / parse / build 実装（spec §7）。
//!
//! - [`FlowgraphFile`]: TOML deserialize 対象のトップレベル構造。
//! - [`parse_flowgraph_file`]: TOML 文字列 → `FlowgraphFile`。
//! - [`load_file`]: 単一ファイル → `FlowgraphProgram`（`NodeRegistry` と `build` まで完走）。
//!
//! 「1 ファイル限定」のショートカット。多ファイル統合は [`super::dir::load_flowgraph_dir`]。

use crate::flowgraph::loader::diagnostic::{
	Diagnostic, DiagnosticCode, LoadError, LoadReport, LoadedNodeMeta, Severity,
};
use crate::flowgraph::loader::reference::parse_port_ref;
use crate::flowgraph::node::{InputMap, NodeSpec};
use crate::flowgraph::registry::{registry, NodeRegistry};
use crate::flowgraph::socket::from_toml_value;
use crate::flowgraph::{FlowgraphBuilder, FlowgraphProgram, PortRef};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------
// File schema（spec §7.2）
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowgraphFile {
	#[serde(default)]
	pub meta: Option<FileMeta>,
	#[serde(default)]
	pub nodes: Vec<NodeEntry>,
	#[serde(default)]
	pub edges: Vec<EdgeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileMeta {
	#[serde(default)]
	pub title: Option<String>,
	#[serde(default)]
	pub description: Option<String>,
	#[serde(default)]
	pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEntry {
	/// ファイル内 unique なノード ID。
	pub id: String,
	/// 登録済みノードの feature 名。
	pub feature: String,
	/// GUI のみ使用。
	#[serde(default)]
	pub position: Option<[f64; 2]>,
	/// ノード properties（`PropertySpec` で型検証）。
	#[serde(default)]
	pub properties: toml::Table,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeEntry {
	/// `"node_id:port"` または `"path::node_id:port"`。
	pub from: String,
	pub to: String,
}

// ---------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------

/// TOML 文字列 → `FlowgraphFile`。
///
/// パースエラーは `LoadError` 1 件にまとめて返す。
pub fn parse_flowgraph_file(src: &str, file_hint: Option<&Path>) -> Result<FlowgraphFile, LoadError> {
	toml::from_str::<FlowgraphFile>(src).map_err(|e| {
		let mut d = Diagnostic::error(DiagnosticCode::TomlParse, format!("TOML パース失敗: {e}"));
		if let Some(p) = file_hint {
			d = d.with_file(p.to_path_buf());
		}
		LoadError::from_single(d)
	})
}

// ---------------------------------------------------------------------
// Single-file entry
// ---------------------------------------------------------------------

/// 単一 `.flowgraph.toml` ファイルをパースしてプログラムを構築する。
///
/// 複数ファイル参照（`path::node_id`）を含むファイルは [`super::dir::load_flowgraph_dir`] 推奨。
/// ここでは `fq_path` 指定のある edge は **現在ファイルの fq を要求**（fq が一致しなければエラー）。
///
/// `file_fq_path_hint` は edge 内で書かれる絶対 fq 名のチェックに使う。
/// 通常は `"main"` や `"graph"` など、呼び出し側で任意に決めてよい。
pub fn load_file(
	path: &Path,
	file_fq_path_hint: Option<&str>,
) -> Result<LoadReport, LoadError> {
	let src = std::fs::read_to_string(path).map_err(|e| {
		LoadError::from_single(
			Diagnostic::error(DiagnosticCode::Io, format!("ファイル読み込み失敗: {e}"))
				.with_file(path.to_path_buf()),
		)
	})?;
	let parsed = parse_flowgraph_file(&src, Some(path))?;

	let fq = file_fq_path_hint.unwrap_or("main").to_string();
	let mut known = HashSet::new();
	known.insert(fq.clone());

	let ctx = BuildContext {
		files: vec![(fq.clone(), path.to_path_buf(), parsed)],
		known_file_fqs: known,
	};
	ctx.build(registry())
}

// ---------------------------------------------------------------------
// Build context（単一ファイル・複数ファイル共通）
// ---------------------------------------------------------------------

/// 複数ファイルの統合ビルド用文脈。単一ファイル loader も同じ経路を通る（files が 1 件）。
pub(crate) struct BuildContext {
	/// (fq_path, ファイルパス, パース済み構造)
	pub files: Vec<(String, PathBuf, FlowgraphFile)>,
	/// 既知の fq path 集合（main 規約解決に使う）。
	pub known_file_fqs: HashSet<String>,
}

impl BuildContext {
	pub(crate) fn build(self, reg: &NodeRegistry) -> Result<LoadReport, LoadError> {
		let mut diagnostics: Vec<Diagnostic> = Vec::new();
		let mut builder = FlowgraphBuilder::new();
		let mut node_meta: HashMap<String, LoadedNodeMeta> = HashMap::new();

		// Pass 1: ノード収集（fq name 生成）。
		// (fq_name, NodeSpec) マップを作り、edge 解決で参照する。
		let mut node_specs: HashMap<String, NodeSpec> = HashMap::new();

		for (file_fq, file_path, file) in &self.files {
			let mut local_ids: HashSet<String> = HashSet::new();

			for node in &file.nodes {
				if node.id.is_empty() {
					diagnostics.push(
						Diagnostic::error(DiagnosticCode::DuplicateNodeId, "node.id が空")
							.with_file(file_path.clone()),
					);
					continue;
				}
				if !local_ids.insert(node.id.clone()) {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::DuplicateNodeId,
							format!("ファイル内で node.id 重複: '{}'", node.id),
						)
						.with_file(file_path.clone())
						.with_node(node.id.clone()),
					);
					continue;
				}

				let fq_name = fq_node_name(file_fq, &node.id);

				// feature → NodeImpl
				let Some(spec) = reg.spec(&node.feature) else {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::UnknownFeature,
							format!("未登録 feature: '{}'", node.feature),
						)
						.with_file(file_path.clone())
						.with_node(node.id.clone())
						.with_hint(node.feature.clone()),
					);
					continue;
				};

				let impl_ = reg.make_impl(&node.feature).expect("spec の直後に make_impl が失敗するのは不整合");

				// プロパティを InputMap に変換
				let properties = match resolve_properties(&spec, &node.properties) {
					Ok((map, diags)) => {
						for d in diags {
							diagnostics.push(
								d.with_file(file_path.clone()).with_node(node.id.clone()),
							);
						}
						map
					}
					Err(diags) => {
						for d in diags {
							diagnostics.push(
								d.with_file(file_path.clone()).with_node(node.id.clone()),
							);
						}
						continue;
					}
				};

				if node_specs.contains_key(&fq_name) {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::DuplicateNodeId,
							format!("fq name 重複: '{}'（複数ファイルで衝突）", fq_name),
						)
						.with_file(file_path.clone())
						.with_node(node.id.clone()),
					);
					continue;
				}

				builder.add_node(fq_name.clone(), impl_, properties.clone());
				node_specs.insert(fq_name.clone(), spec.clone());
				node_meta.insert(
					fq_name,
					LoadedNodeMeta {
						feature: node.feature.clone(),
						file: file_path.clone(),
						position: node.position,
						properties,
					},
				);
			}
		}

		// Pass 2: エッジ解決
		let ctx_files: HashSet<String> = self.known_file_fqs.clone();
		for (file_fq, file_path, file) in &self.files {
			let resolve_ctx = crate::flowgraph::loader::reference::ResolveContext {
				current_file_fq: file_fq.clone(),
				known_file_fqs: ctx_files.clone(),
			};

			for (idx, edge) in file.edges.iter().enumerate() {
				let edge_hint = format!("[[edges]][{idx}]");

				let parsed_from = match parse_port_ref(&edge.from) {
					Ok(v) => v,
					Err(e) => {
						diagnostics.push(
							Diagnostic::error(
								DiagnosticCode::InvalidPortRef,
								format!("edge.from 不正: {e}"),
							)
							.with_file(file_path.clone())
							.with_hint(edge_hint.clone()),
						);
						continue;
					}
				};
				let parsed_to = match parse_port_ref(&edge.to) {
					Ok(v) => v,
					Err(e) => {
						diagnostics.push(
							Diagnostic::error(
								DiagnosticCode::InvalidPortRef,
								format!("edge.to 不正: {e}"),
							)
							.with_file(file_path.clone())
							.with_hint(edge_hint.clone()),
						);
						continue;
					}
				};

				let from_fq_path = match crate::flowgraph::loader::reference::resolve_fq_ref(
					&parsed_from,
					&resolve_ctx,
				) {
					Ok(v) => v,
					Err(e) => {
						diagnostics.push(
							Diagnostic::error(DiagnosticCode::InvalidPortRef, e)
								.with_file(file_path.clone())
								.with_hint(edge_hint.clone()),
						);
						continue;
					}
				};
				let to_fq_path = match crate::flowgraph::loader::reference::resolve_fq_ref(
					&parsed_to,
					&resolve_ctx,
				) {
					Ok(v) => v,
					Err(e) => {
						diagnostics.push(
							Diagnostic::error(DiagnosticCode::InvalidPortRef, e)
								.with_file(file_path.clone())
								.with_hint(edge_hint.clone()),
						);
						continue;
					}
				};

				let from_fq_name = fq_node_name(&from_fq_path, &parsed_from.node_id);
				let to_fq_name = fq_node_name(&to_fq_path, &parsed_to.node_id);

				let Some(from_spec) = node_specs.get(&from_fq_name) else {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::UnresolvedNodeRef,
							format!("edge.from が解決不能: '{}'", from_fq_name),
						)
						.with_file(file_path.clone())
						.with_hint(edge_hint.clone()),
					);
					continue;
				};
				let Some(to_spec) = node_specs.get(&to_fq_name) else {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::UnresolvedNodeRef,
							format!("edge.to が解決不能: '{}'", to_fq_name),
						)
						.with_file(file_path.clone())
						.with_hint(edge_hint.clone()),
					);
					continue;
				};

				let Some(from_port) = from_spec.find_output(&parsed_from.port) else {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::UnknownPort,
							format!(
								"未知出力ポート: '{}' にポート '{}' が無い",
								from_fq_name, parsed_from.port
							),
						)
						.with_file(file_path.clone())
						.with_hint(edge_hint.clone()),
					);
					continue;
				};
				let Some(to_port) = to_spec.find_input(&parsed_to.port) else {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::UnknownPort,
							format!(
								"未知入力ポート: '{}' にポート '{}' が無い",
								to_fq_name, parsed_to.port
							),
						)
						.with_file(file_path.clone())
						.with_hint(edge_hint.clone()),
					);
					continue;
				};

				let is_exec = from_port.is_exec && to_port.is_exec;
				if from_port.is_exec != to_port.is_exec {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::EngineBuild,
							format!(
								"exec と data を跨ぐエッジ: '{}' → '{}'",
								edge.from, edge.to
							),
						)
						.with_file(file_path.clone())
						.with_hint(edge_hint.clone()),
					);
					continue;
				}

				let from_pr = PortRef::new(from_fq_name, parsed_from.port.clone());
				let to_pr = PortRef::new(to_fq_name, parsed_to.port.clone());
				if is_exec {
					builder.connect_exec(from_pr, to_pr);
				} else {
					builder.connect(from_pr, to_pr);
				}
			}
		}

		if diagnostics.iter().any(|d| d.severity == Severity::Error) {
			return Err(LoadError::new(diagnostics));
		}

		// Pass 3: engine build
		let program: FlowgraphProgram = builder.build().map_err(|e| {
			let mut ds = diagnostics.clone();
			ds.push(Diagnostic::error(
				DiagnosticCode::EngineBuild,
				format!("engine build 失敗: {e}"),
			));
			LoadError::new(ds)
		})?;

		Ok(LoadReport {
			program,
			diagnostics,
			node_meta,
		})
	}
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// fq path + node id → 組み合わせ fq name（engine の node_id）。
pub(crate) fn fq_node_name(fq_path: &str, node_id: &str) -> String {
	if fq_path.is_empty() {
		node_id.to_string()
	} else {
		format!("{fq_path}::{node_id}")
	}
}

/// TOML プロパティテーブルを `InputMap` に変換する。
///
/// - 未知プロパティは warning を積んでスキップ。
/// - 必須プロパティ（`PropertySpec.required`）未指定は error。
/// - 型不一致は error。
/// - 型一致の場合、欠けているプロパティは `PropertySpec.default` を充てる。
///
/// 成功時は (InputMap, warnings) を返す。失敗時は Err(errors)。
fn resolve_properties(
	spec: &NodeSpec,
	table: &toml::Table,
) -> Result<(InputMap, Vec<Diagnostic>), Vec<Diagnostic>> {
	let mut map: InputMap = InputMap::new();
	let mut diags: Vec<Diagnostic> = Vec::new();
	let mut errors_occurred = false;

	let declared: HashSet<&str> = spec.properties.iter().map(|p| p.name.as_str()).collect();

	for (key, value) in table {
		if !declared.contains(key.as_str()) {
			diags.push(Diagnostic::warning(
				DiagnosticCode::UnknownProperty,
				format!("未知プロパティ: '{key}'（feature '{}' には存在しない）", spec.feature),
			));
			continue;
		}
		let prop = spec.find_property(key).unwrap();
		match from_toml_value(&prop.ty, value) {
			Ok(v) => {
				map.insert(key.clone(), v);
			}
			Err(e) => {
				diags.push(Diagnostic::error(
					DiagnosticCode::PropertyTypeMismatch,
					format!("プロパティ '{key}' の型不一致: {e}"),
				));
				errors_occurred = true;
			}
		}
	}

	// デフォルト補完 + 必須チェック
	for prop in &spec.properties {
		if map.contains_key(&prop.name) {
			continue;
		}
		if prop.required {
			diags.push(Diagnostic::error(
				DiagnosticCode::MissingRequiredProperty,
				format!("必須プロパティ未指定: '{}'", prop.name),
			));
			errors_occurred = true;
			continue;
		}
		if let Some(v) = prop.default.to_socket_value(&prop.ty) {
			map.insert(prop.name.clone(), v);
		}
	}

	if errors_occurred {
		Err(diags)
	} else {
		Ok((map, diags))
	}
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	fn write_tmp(name: &str, contents: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!(
			"vac-flowgraph-loader-{}-{}",
			std::process::id(),
			rand_suffix()
		));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join(name);
		std::fs::write(&path, contents).unwrap();
		path
	}

	fn rand_suffix() -> String {
		use std::time::{SystemTime, UNIX_EPOCH};
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map(|d| d.as_nanos())
			.unwrap_or(0)
			.to_string()
	}

	#[test]
	fn parse_minimal_file() {
		let src = r#"
			[meta]
			title = "test"

			[[nodes]]
			id = "lit"
			feature = "flowgraph.literal.string"
			properties.value = "hello"
		"#;
		let f = parse_flowgraph_file(src, None).unwrap();
		assert_eq!(f.meta.unwrap().title.unwrap(), "test");
		assert_eq!(f.nodes.len(), 1);
		assert_eq!(f.nodes[0].id, "lit");
		assert_eq!(f.nodes[0].feature, "flowgraph.literal.string");
	}

	#[test]
	fn load_single_file_literal_and_log() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "hi"

				[[nodes]]
				id = "logger"
				feature = "flowgraph.util.log"

				[[edges]]
				from = "lit:value"
				to = "logger:value"
			"#,
		);
		let report = load_file(&path, None).expect("should load");
		assert!(report.diagnostics.iter().all(|d| d.severity != Severity::Error));
		assert!(report.node_meta.contains_key("main::lit"));
		assert!(report.node_meta.contains_key("main::logger"));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_unknown_feature_errors() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "x"
				feature = "flowgraph.not.a.feature"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err
			.errors()
			.any(|d| d.code == DiagnosticCode::UnknownFeature));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_unknown_port_errors() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "hi"

				[[nodes]]
				id = "logger"
				feature = "flowgraph.util.log"

				[[edges]]
				from = "lit:value"
				to = "logger:does_not_exist"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err.errors().any(|d| d.code == DiagnosticCode::UnknownPort));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_unresolved_node_ref_errors() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "logger"
				feature = "flowgraph.util.log"

				[[edges]]
				from = "ghost:value"
				to = "logger:text"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err
			.errors()
			.any(|d| d.code == DiagnosticCode::UnresolvedNodeRef));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_duplicate_node_id_errors() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "a"
				feature = "flowgraph.literal.string"

				[[nodes]]
				id = "a"
				feature = "flowgraph.literal.string"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err
			.errors()
			.any(|d| d.code == DiagnosticCode::DuplicateNodeId));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_property_type_mismatch_errors() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.int"
				properties.value = "not_an_int"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err
			.errors()
			.any(|d| d.code == DiagnosticCode::PropertyTypeMismatch));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_unknown_property_is_warning_only() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "ok"
				properties.bogus = "nonsense"
			"#,
		);
		let report = load_file(&path, None).expect("should succeed with warning");
		assert!(report
			.diagnostics
			.iter()
			.any(|d| d.code == DiagnosticCode::UnknownProperty && d.severity == Severity::Warning));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_edge_with_bad_port_ref_errors() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"

				[[edges]]
				from = "lit_value_no_colon"
				to = "lit:value"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err
			.errors()
			.any(|d| d.code == DiagnosticCode::InvalidPortRef));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	// η 仕様書 §10.3 の "Example flowgraph 実走" を smoke test として固定化する。
	// 実データ流し込みまでは行わず、load + build が通ることで 95% の回帰を拾う想定。
	// engine レベルの動作は `flowgraph::nodes::dictionary::tests` でカバー済み。
	#[test]
	fn load_example_basic_replace_dictionary() {
		let path = Path::new("flowgraph.example/dictionary/basic-replace.flowgraph.toml");
		let report = load_file(path, None).expect("basic-replace example should load");
		assert!(
			report.diagnostics.iter().all(|d| d.severity != Severity::Error),
			"unexpected errors: {:?}",
			report.diagnostics,
		);
		for id in ["dict_path", "dict_mode", "load", "in", "replace", "log_out"] {
			let fq = format!("main::{id}");
			assert!(
				report.node_meta.contains_key(&fq),
				"node `{fq}` not resolved; got keys: {:?}",
				report.node_meta.keys().collect::<Vec<_>>(),
			);
		}
	}

	#[test]
	fn load_example_command_dispatch() {
		let path = Path::new("flowgraph.example/dictionary/command-dispatch.flowgraph.toml");
		let report = load_file(path, None).expect("command-dispatch example should load");
		assert!(
			report.diagnostics.iter().all(|d| d.severity != Severity::Error),
			"unexpected errors: {:?}",
			report.diagnostics,
		);
		for id in ["in", "cmd_path", "cmd_mode", "load_cmds", "match", "log_matched", "log_plain"]
		{
			let fq = format!("main::{id}");
			assert!(
				report.node_meta.contains_key(&fq),
				"node `{fq}` not resolved; got keys: {:?}",
				report.node_meta.keys().collect::<Vec<_>>(),
			);
		}
	}
}
