//! 単一 `*.flowgraph.toml` ファイルの schema / parse / build 実装（spec §7）。
//!
//! - [`FlowgraphFile`]: TOML deserialize 対象のトップレベル構造。
//! - [`parse_flowgraph_file`]: TOML 文字列 → `FlowgraphFile`。
//! - [`load_file`]: 単一ファイル → `FlowgraphProgram`（`NodeRegistry` と `build` まで完走）。
//!
//! 「1 ファイル限定」のショートカット。多ファイル統合は [`super::dir::load_flowgraph_dir`]。

use crate::flowgraph::loader::diagnostic::{
	Diagnostic, DiagnosticCode, FlowgraphFileActivationMeta, GraphCapabilitySummary, GraphSignature, GraphSignatureFile,
	GraphSignaturePort, GraphSignatureTrigger, LoadError, LoadReport, LoadedNodeMeta, PackageLockEntry, PackageManifestSummary, Severity,
};
use crate::flowgraph::loader::reference::parse_port_ref;
use crate::flowgraph::node::{InputMap, NodeSpec, PortSpec};
use crate::flowgraph::registry::{registry, NodeRegistry};
use crate::flowgraph::socket::{from_toml_value, SocketValue};
use crate::flowgraph::{FlowgraphBuilder, FlowgraphProgram, PortRef};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------
// File schema（spec §7.2）
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowgraphFile {
	#[serde(default)]
	pub meta: Option<FileMeta>,
	#[serde(default)]
	pub package: Option<FlowgraphPackageManifest>,
	#[serde(default)]
	pub nodes: Vec<NodeEntry>,
	#[serde(default)]
	pub edges: Vec<EdgeEntry>,
	/// Phase λ: ユーザ定義閉集合（`[[enums]]`）。未指定は空。
	#[serde(default)]
	pub enums: Vec<FlowgraphEnumDef>,
	/// Phase υ: GUI 上の編集グループ。engine 実行には影響しない。
	#[serde(default)]
	pub groups: Vec<FlowgraphGroupDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowgraphPackageManifest {
	#[serde(default)]
	pub id: Option<String>,
	#[serde(default)]
	pub version: Option<String>,
	#[serde(default)]
	pub exports: Vec<String>,
	#[serde(default)]
	pub dependencies: BTreeMap<String, String>,
}

/// TOML `[[enums]]` 1 行相当。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowgraphEnumDef {
	pub id: String,
	/// 将来用。v0 は `string`（または省略）のみ扱う。
	#[serde(default)]
	pub primitive: Option<String>,
	#[serde(default)]
	pub variants: Vec<String>,
}

/// TOML `[[groups]]` 1 行相当。Flowgraph editor の視覚的なまとまりを保存する。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowgraphGroupDef {
	pub id: String,
	#[serde(default)]
	pub label: Option<String>,
	#[serde(default)]
	pub node_ids: Vec<String>,
	#[serde(default)]
	pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
	#[serde(default)]
	pub title: Option<String>,
	#[serde(default)]
	pub description: Option<String>,
	#[serde(default)]
	pub tags: Option<Vec<String>>,
	#[serde(default)]
	pub author: Option<String>,
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub version: Option<String>,
	#[serde(default)]
	pub license: Option<String>,
	#[serde(default)]
	pub repos: Option<String>,
	/// Phase λ: 依存先フローの fq（`sub/pkg/graph` 形式、拡張子なし）。
	#[serde(default)]
	pub library_uses: Option<Vec<String>>,
	/// RM-3: `conf` の `modes.*.flowgraph_groups` が参照するグループ名。
	#[serde(default)]
	pub mode_groups: Vec<String>,
	/// RM-3: mode 未適用時の既定。省略時は roadmap どおり `true`（既存ファイル互換）。
	#[serde(default = "crate::utility::bool_true")]
	pub default_enabled: bool,
}

impl Default for FileMeta {
	fn default() -> Self {
		Self {
			title: None,
			description: None,
			tags: None,
			author: None,
			name: None,
			version: None,
			license: None,
			repos: None,
			library_uses: None,
			mode_groups: Vec::new(),
			default_enabled: true,
		}
	}
}

impl FileMeta {
	/// RM-3: Mode Manager が参照するサマリ（`[meta]` 省略時は [`FlowgraphFileActivationMeta::default`] と同値）。
	pub fn activation_meta(&self) -> FlowgraphFileActivationMeta {
		FlowgraphFileActivationMeta {
			mode_groups: self.mode_groups.clone(),
			default_enabled: self.default_enabled,
		}
	}
}

/// `[meta]` 省略時を含め、ファイル単位の activation メタを返す。
pub fn file_activation_meta(file: &FlowgraphFile) -> FlowgraphFileActivationMeta {
	file.meta.as_ref().map(FileMeta::activation_meta).unwrap_or_default()
}

fn validate_file_activation_meta(file: &FlowgraphFile, file_path: &Path, diagnostics: &mut Vec<Diagnostic>) {
	let Some(meta) = file.meta.as_ref() else {
		return;
	};
	for (i, g) in meta.mode_groups.iter().enumerate() {
		if g.trim().is_empty() {
			diagnostics.push(
				Diagnostic::error(DiagnosticCode::InvalidModeMetadata, format!("[meta].mode_groups[{i}] が空です"))
					.with_file(file_path.to_path_buf())
					.with_hint("mode_groups"),
			);
		}
	}
}

/// Phase λ: `author` / `name` / `version` がすべて非空のときの表示用安定 ID。
pub fn normalized_library_id(meta: &FileMeta) -> Option<String> {
	let author = meta.author.as_deref()?.trim();
	let name = meta.name.as_deref()?.trim();
	let ver = meta.version.as_deref()?.trim();
	if author.is_empty() || name.is_empty() || ver.is_empty() {
		return None;
	}
	fn norm_token(s: &str) -> String {
		let t: String = s
			.chars()
			.map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
			.collect();
		t.trim_matches('_').to_string()
	}
	let a = norm_token(author);
	let n = norm_token(name);
	let v = norm_token(ver);
	if a.is_empty() || n.is_empty() || v.is_empty() {
		return None;
	}
	Some(format!("{a}::{n}::{v}"))
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
pub fn load_file(path: &Path, file_fq_path_hint: Option<&str>) -> Result<LoadReport, LoadError> {
	let src = std::fs::read_to_string(path).map_err(|e| {
		LoadError::from_single(Diagnostic::error(DiagnosticCode::Io, format!("ファイル読み込み失敗: {e}")).with_file(path.to_path_buf()))
	})?;
	let parsed = parse_flowgraph_file(&src, Some(path))?;

	let fq = file_fq_path_hint.unwrap_or("main").to_string();
	let mut known = HashSet::new();
	known.insert(fq.clone());

	let ctx = BuildContext {
		files: vec![(fq.clone(), path.to_path_buf(), parsed)],
		source_digests: HashMap::from([(fq.clone(), package_source_digest(&src))]),
		known_file_fqs: known,
	};
	ctx.build(registry())
}

// ---------------------------------------------------------------------
// Build context（単一ファイル・複数ファイル共通）
// ---------------------------------------------------------------------

fn validate_enum_definitions(file: &FlowgraphFile, file_path: &Path, diagnostics: &mut Vec<Diagnostic>) {
	let mut seen: HashSet<String> = HashSet::new();
	for (idx, e) in file.enums.iter().enumerate() {
		let hint = format!("[[enums]][{idx}]");
		if e.id.trim().is_empty() {
			diagnostics.push(
				Diagnostic::error(DiagnosticCode::InvalidEnumDefinition, "enums.id が空")
					.with_file(file_path.to_path_buf())
					.with_hint(hint.clone()),
			);
			continue;
		}
		if !seen.insert(e.id.clone()) {
			diagnostics.push(
				Diagnostic::error(DiagnosticCode::DuplicateEnumId, format!("enums.id 重複: '{}'", e.id))
					.with_file(file_path.to_path_buf())
					.with_hint(hint.clone()),
			);
		}
		if e.variants.is_empty() {
			diagnostics.push(
				Diagnostic::error(DiagnosticCode::InvalidEnumDefinition, format!("enums '{}' の variants が空", e.id))
					.with_file(file_path.to_path_buf())
					.with_hint(hint.clone()),
			);
			continue;
		}
		if let Some(ref prim) = e.primitive {
			let p = prim.trim();
			if !p.is_empty() && p != "string" {
				diagnostics.push(
					Diagnostic::warning(
						DiagnosticCode::PropertyTypeMismatch,
						format!("enums '{}' の primitive='{prim}' は v0 で string のみサポート", e.id),
					)
					.with_file(file_path.to_path_buf())
					.with_hint(hint.clone()),
				);
			}
		}
	}
}

/// 複数ファイルの統合ビルド用文脈。単一ファイル loader も同じ経路を通る（files が 1 件）。
pub(crate) struct BuildContext {
	/// (fq_path, ファイルパス, パース済み構造)
	pub files: Vec<(String, PathBuf, FlowgraphFile)>,
	/// fq_path ごとの raw source content digest。lockfile preview 用の read-only metadata。
	pub source_digests: HashMap<String, String>,
	/// 既知の fq path 集合（main 規約解決に使う）。
	pub known_file_fqs: HashSet<String>,
}

impl BuildContext {
	pub(crate) fn build(self, reg: &NodeRegistry) -> Result<LoadReport, LoadError> {
		let mut diagnostics: Vec<Diagnostic> = Vec::new();
		for (_, file_path, file) in &self.files {
			validate_enum_definitions(file, file_path, &mut diagnostics);
		}
		for (_, file_path, file) in &self.files {
			validate_file_activation_meta(file, file_path, &mut diagnostics);
		}
		let file_activation: HashMap<String, FlowgraphFileActivationMeta> =
			self.files.iter().map(|(fq, _, f)| (fq.clone(), file_activation_meta(f))).collect();

		let mut builder = FlowgraphBuilder::new();
		let mut node_meta: HashMap<String, LoadedNodeMeta> = HashMap::new();

		// Pass 1: ノード収集（fq name 生成）。
		// (fq_name, NodeSpec) マップを作り、edge 解決で参照する。
		let mut node_specs: HashMap<String, NodeSpec> = HashMap::new();

		for (file_fq, file_path, file) in &self.files {
			let mut local_ids: HashSet<String> = HashSet::new();

			for node in &file.nodes {
				if node.id.is_empty() {
					diagnostics.push(Diagnostic::error(DiagnosticCode::DuplicateNodeId, "node.id が空").with_file(file_path.clone()));
					continue;
				}
				if !local_ids.insert(node.id.clone()) {
					diagnostics.push(
						Diagnostic::error(DiagnosticCode::DuplicateNodeId, format!("ファイル内で node.id 重複: '{}'", node.id))
							.with_file(file_path.clone())
							.with_node(node.id.clone()),
					);
					continue;
				}

				let fq_name = fq_node_name(file_fq, &node.id);

				// feature → NodeImpl
				let Some(spec) = reg.spec(&node.feature) else {
					diagnostics.push(
						Diagnostic::error(DiagnosticCode::UnknownFeature, format!("未登録 feature: '{}'", node.feature))
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
							diagnostics.push(d.with_file(file_path.clone()).with_node(node.id.clone()));
						}
						map
					}
					Err(diags) => {
						for d in diags {
							diagnostics.push(d.with_file(file_path.clone()).with_node(node.id.clone()));
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
		let mut connected_inputs: BTreeSet<(String, String)> = BTreeSet::new();
		let mut connected_outputs: BTreeSet<(String, String)> = BTreeSet::new();
		let mut edge_count = 0;
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
							Diagnostic::error(DiagnosticCode::InvalidPortRef, format!("edge.from 不正: {e}"))
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
							Diagnostic::error(DiagnosticCode::InvalidPortRef, format!("edge.to 不正: {e}"))
								.with_file(file_path.clone())
								.with_hint(edge_hint.clone()),
						);
						continue;
					}
				};

				let from_fq_path = match crate::flowgraph::loader::reference::resolve_fq_ref(&parsed_from, &resolve_ctx) {
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
				let to_fq_path = match crate::flowgraph::loader::reference::resolve_fq_ref(&parsed_to, &resolve_ctx) {
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
						Diagnostic::error(DiagnosticCode::UnresolvedNodeRef, format!("edge.to が解決不能: '{}'", to_fq_name))
							.with_file(file_path.clone())
							.with_hint(edge_hint.clone()),
					);
					continue;
				};

				let Some(from_port) = from_spec.find_output(&parsed_from.port) else {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::UnknownPort,
							format!("未知出力ポート: '{}' にポート '{}' が無い", from_fq_name, parsed_from.port),
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
							format!("未知入力ポート: '{}' にポート '{}' が無い", to_fq_name, parsed_to.port),
						)
						.with_file(file_path.clone())
						.with_hint(edge_hint.clone()),
					);
					continue;
				};

				// Phase λ: 閉集合 string 入力 ← string リテラル のとき、プロパティ value を検証。
				if !from_port.is_exec && !to_port.is_exec {
					if let Some(allowed) = to_port.closed_string_variants.as_ref().filter(|v| !v.is_empty()) {
						if let Some(meta_from) = node_meta.get(&from_fq_name) {
							if meta_from.feature == "flowgraph.literal.string" {
								if let Some(SocketValue::String(s)) = meta_from.properties.get("value") {
									if !allowed.iter().any(|v| v == s) {
										diagnostics.push(
											Diagnostic::error(
												DiagnosticCode::ClosedStringLiteralOutOfEnum,
												format!(
													"文字列リテラルの値 '{}' が port '{}' の閉集合に無い: {:?}",
													s, parsed_to.port, allowed
												),
											)
											.with_file(file_path.clone())
											.with_node(parsed_from.node_id.clone())
											.with_hint(edge_hint.clone()),
										);
									}
								}
							}
						}
					}
				}

				let is_exec = from_port.is_exec && to_port.is_exec;
				if from_port.is_exec != to_port.is_exec {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::EngineBuild,
							format!("exec と data を跨ぐエッジ: '{}' → '{}'", edge.from, edge.to),
						)
						.with_file(file_path.clone())
						.with_hint(edge_hint.clone()),
					);
					continue;
				}

				let from_pr = PortRef::new(from_fq_name, parsed_from.port.clone());
				let to_pr = PortRef::new(to_fq_name, parsed_to.port.clone());
				connected_outputs.insert((from_pr.node.clone(), from_pr.port.clone()));
				connected_inputs.insert((to_pr.node.clone(), to_pr.port.clone()));
				edge_count += 1;
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
			ds.push(Diagnostic::error(DiagnosticCode::EngineBuild, format!("engine build 失敗: {e}")));
			LoadError::new(ds)
		})?;

		let capability_summary = crate::flowgraph::loader::diagnostic::GraphCapabilitySummary::from_node_meta(reg, &node_meta);
		let package_manifests = build_package_manifest_summary(&self.files, &self.source_digests);
		let package_dependency_order = build_package_dependency_order(&package_manifests);
		let package_lock_preview = build_package_lock_preview(&package_manifests, &package_dependency_order);
		let package_lock_preview_digest = build_package_lock_preview_digest(&package_lock_preview);
		let graph_signature = build_graph_signature(
			reg,
			&self.files,
			&node_meta,
			&node_specs,
			&connected_inputs,
			&connected_outputs,
			edge_count,
			&capability_summary,
		);

		Ok(LoadReport {
			program,
			diagnostics,
			node_meta,
			graph_signature,
			package_manifests,
			package_dependency_order,
			package_lock_preview,
			package_lock_preview_digest,
			capability_summary,
			file_activation,
		})
	}
}

fn build_package_manifest_summary(
	files: &[(String, PathBuf, FlowgraphFile)],
	source_digests: &HashMap<String, String>,
) -> Vec<PackageManifestSummary> {
	let mut manifests: Vec<PackageManifestSummary> = files
		.iter()
		.filter_map(|(fq, _, file)| {
			let package = file.package.as_ref()?;
			let source_digest = source_digests.get(fq).cloned().unwrap_or_else(|| package_source_digest(""));
			Some(PackageManifestSummary {
				source_fq: fq.clone(),
				id: package.id.clone(),
				version: package.version.clone(),
				exports: package.exports.clone(),
				dependencies: package.dependencies.clone(),
				source_digest: Some(source_digest),
			})
		})
		.collect();
	manifests.sort_by(|a, b| a.source_fq.cmp(&b.source_fq));
	manifests
}

fn build_package_dependency_order(package_manifests: &[PackageManifestSummary]) -> Vec<String> {
	let known_ids: BTreeSet<String> = package_manifests.iter().filter_map(|manifest| manifest.id.clone()).collect();
	let mut dependencies_by_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for manifest in package_manifests {
		let Some(id) = manifest.id.as_ref() else {
			continue;
		};
		let dependencies = manifest
			.dependencies
			.keys()
			.filter(|dependency_id| known_ids.contains(*dependency_id))
			.cloned()
			.collect();
		dependencies_by_id.insert(id.clone(), dependencies);
	}

	fn visit(
		id: &str,
		dependencies_by_id: &BTreeMap<String, Vec<String>>,
		visiting: &mut BTreeSet<String>,
		visited: &mut BTreeSet<String>,
		order: &mut Vec<String>,
	) {
		if visited.contains(id) || !visiting.insert(id.to_string()) {
			return;
		}
		for dependency_id in dependencies_by_id.get(id).into_iter().flatten() {
			visit(dependency_id, dependencies_by_id, visiting, visited, order);
		}
		visiting.remove(id);
		if visited.insert(id.to_string()) {
			order.push(id.to_string());
		}
	}

	let mut visiting: BTreeSet<String> = BTreeSet::new();
	let mut visited: BTreeSet<String> = BTreeSet::new();
	let mut order: Vec<String> = Vec::new();
	for id in dependencies_by_id.keys() {
		visit(id, &dependencies_by_id, &mut visiting, &mut visited, &mut order);
	}
	order
}

fn build_package_lock_preview(package_manifests: &[PackageManifestSummary], package_dependency_order: &[String]) -> Vec<PackageLockEntry> {
	let manifests_by_id: BTreeMap<String, &PackageManifestSummary> = package_manifests
		.iter()
		.filter_map(|manifest| manifest.id.as_ref().map(|id| (id.clone(), manifest)))
		.collect();

	package_dependency_order
		.iter()
		.filter_map(|id| {
			let manifest = manifests_by_id.get(id)?;
			let source_digest = manifest.source_digest.clone().unwrap_or_else(|| package_source_digest(""));
			let digest = package_lock_entry_digest(
				id,
				manifest.version.as_deref(),
				&manifest.source_fq,
				&source_digest,
				&manifest.dependencies,
			);
			Some(PackageLockEntry {
				id: id.clone(),
				version: manifest.version.clone(),
				source_fq: manifest.source_fq.clone(),
				source_digest,
				digest,
				dependencies: manifest.dependencies.clone(),
			})
		})
		.collect()
}

fn build_package_lock_preview_digest(entries: &[PackageLockEntry]) -> Option<String> {
	if entries.is_empty() {
		return None;
	}
	let mut bytes: Vec<u8> = Vec::new();
	bytes.extend_from_slice(b"vac.package-lock-preview.v1\0");
	for entry in entries {
		bytes.extend_from_slice(b"entry\0");
		bytes.extend_from_slice(entry.id.as_bytes());
		bytes.push(0);
		bytes.extend_from_slice(entry.digest.as_bytes());
		bytes.push(0);
	}
	Some(format!("b3:{}", hex_encode_32(blake3::hash(&bytes).as_bytes())))
}

pub(crate) fn package_source_digest(src: &str) -> String {
	format!("b3:{}", hex_encode_32(blake3::hash(src.as_bytes()).as_bytes()))
}

fn package_lock_entry_digest(
	id: &str,
	version: Option<&str>,
	source_fq: &str,
	source_digest: &str,
	dependencies: &BTreeMap<String, String>,
) -> String {
	let mut bytes: Vec<u8> = Vec::new();
	bytes.extend_from_slice(b"vac.package-lock-entry.v1\0");
	bytes.extend_from_slice(b"id\0");
	bytes.extend_from_slice(id.as_bytes());
	bytes.push(0);
	bytes.extend_from_slice(b"version\0");
	bytes.extend_from_slice(version.unwrap_or("").as_bytes());
	bytes.push(0);
	bytes.extend_from_slice(b"source_fq\0");
	bytes.extend_from_slice(source_fq.as_bytes());
	bytes.push(0);
	bytes.extend_from_slice(b"source_digest\0");
	bytes.extend_from_slice(source_digest.as_bytes());
	bytes.push(0);
	for (dependency_id, requirement) in dependencies {
		bytes.extend_from_slice(b"dependency\0");
		bytes.extend_from_slice(dependency_id.as_bytes());
		bytes.push(0);
		bytes.extend_from_slice(requirement.as_bytes());
		bytes.push(0);
	}
	format!("b3:{}", hex_encode_32(blake3::hash(&bytes).as_bytes()))
}

fn hex_encode_32(bytes: &[u8; 32]) -> String {
	const HEX: &[u8; 16] = b"0123456789abcdef";
	let mut out = String::with_capacity(64);
	for byte in bytes {
		out.push(HEX[(byte >> 4) as usize] as char);
		out.push(HEX[(byte & 0x0f) as usize] as char);
	}
	out
}

fn build_graph_signature(
	reg: &NodeRegistry,
	files: &[(String, PathBuf, FlowgraphFile)],
	node_meta: &HashMap<String, LoadedNodeMeta>,
	node_specs: &HashMap<String, NodeSpec>,
	connected_inputs: &BTreeSet<(String, String)>,
	connected_outputs: &BTreeSet<(String, String)>,
	edge_count: usize,
	capability_summary: &GraphCapabilitySummary,
) -> GraphSignature {
	let mut files_meta: Vec<GraphSignatureFile> = files
		.iter()
		.map(|(fq, _, file)| {
			let activation = file_activation_meta(file);
			GraphSignatureFile {
				fq: fq.clone(),
				title: file.meta.as_ref().and_then(|meta| meta.title.clone()),
				description: file.meta.as_ref().and_then(|meta| meta.description.clone()),
				library_id: file.meta.as_ref().and_then(normalized_library_id),
				package_id: file.package.as_ref().and_then(|package| package.id.clone()),
				package_version: file.package.as_ref().and_then(|package| package.version.clone()),
				package_exports: file.package.as_ref().map(|package| package.exports.clone()).unwrap_or_default(),
				mode_groups: activation.mode_groups,
				default_enabled: activation.default_enabled,
			}
		})
		.collect();
	files_meta.sort_by(|a, b| a.fq.cmp(&b.fq));

	let mut node_ids: Vec<&String> = node_specs.keys().collect();
	node_ids.sort();
	let mut boundary_inputs = Vec::new();
	let mut boundary_outputs = Vec::new();
	let mut external_triggers = Vec::new();
	for node in node_ids {
		let Some(spec) = node_specs.get(node) else {
			continue;
		};
		let Some(meta) = node_meta.get(node) else {
			continue;
		};

		for port in &spec.inputs {
			if !connected_inputs.contains(&(node.clone(), port.name.clone())) {
				boundary_inputs.push(graph_signature_port(node, &meta.feature, port));
			}
		}
		for port in &spec.outputs {
			if !connected_outputs.contains(&(node.clone(), port.name.clone())) {
				boundary_outputs.push(graph_signature_port(node, &meta.feature, port));
			}
		}

		let trigger_kind = if reg.is_control_triggerable(&meta.feature) {
			Some("control")
		} else if meta.feature.starts_with("flowgraph.ingress.") {
			Some("ingress")
		} else {
			None
		};
		if let Some(trigger_kind) = trigger_kind {
			let exec_inputs = spec
				.inputs
				.iter()
				.filter(|port| port.is_exec)
				.map(|port| port.name.clone())
				.collect();
			let data_inputs = spec
				.inputs
				.iter()
				.filter(|port| !port.is_exec)
				.map(|port| graph_signature_port(node, &meta.feature, port))
				.collect();
			external_triggers.push(GraphSignatureTrigger {
				node: node.clone(),
				feature: meta.feature.clone(),
				trigger_kind: trigger_kind.into(),
				exec_inputs,
				data_inputs,
			});
		}
	}

	GraphSignature {
		version: 1,
		kind: "flowgraph".into(),
		file_count: files_meta.len(),
		node_count: node_meta.len(),
		edge_count,
		effectful_node_count: capability_summary.effectful_node_count,
		stateful_node_count: capability_summary.stateful_node_count,
		snapshot_supported_state_node_count: capability_summary.snapshot_supported_state_node_count,
		restore_supported_state_node_count: capability_summary.restore_supported_state_node_count,
		required_capabilities: capability_summary.capabilities.clone(),
		files: files_meta,
		external_triggers,
		boundary_inputs,
		boundary_outputs,
	}
}

fn graph_signature_port(node: &str, feature: &str, port: &PortSpec) -> GraphSignaturePort {
	GraphSignaturePort {
		node: node.to_string(),
		feature: feature.to_string(),
		port: port.name.clone(),
		label: port.label.clone(),
		ty: port.ty.to_string(),
		direction: match port.direction {
			crate::flowgraph::node::PortDirection::Input => "input".into(),
			crate::flowgraph::node::PortDirection::Output => "output".into(),
		},
		exec: port.is_exec,
		optional: port.optional,
		multi: port.multi,
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
fn resolve_properties(spec: &NodeSpec, table: &toml::Table) -> Result<(InputMap, Vec<Diagnostic>), Vec<Diagnostic>> {
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
		let dir = std::env::temp_dir().join(format!("vac-flowgraph-loader-{}-{}", std::process::id(), rand_suffix()));
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

			[package]
			id = "example.echo"
			version = "0.1.0"
			exports = ["main"]

			[[nodes]]
			id = "lit"
			feature = "flowgraph.literal.string"
			properties.value = "hello"
		"#;
		let f = parse_flowgraph_file(src, None).unwrap();
		assert_eq!(f.meta.unwrap().title.unwrap(), "test");
		let package = f.package.unwrap();
		assert_eq!(package.id.as_deref(), Some("example.echo"));
		assert_eq!(package.version.as_deref(), Some("0.1.0"));
		assert_eq!(package.exports, vec!["main".to_string()]);
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
		assert!(err.errors().any(|d| d.code == DiagnosticCode::UnknownFeature));
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
		assert!(err.errors().any(|d| d.code == DiagnosticCode::UnresolvedNodeRef));
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
		assert!(err.errors().any(|d| d.code == DiagnosticCode::DuplicateNodeId));
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
		assert!(err.errors().any(|d| d.code == DiagnosticCode::PropertyTypeMismatch));
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
		assert!(err.errors().any(|d| d.code == DiagnosticCode::InvalidPortRef));
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

	// .local/ 配下の個人環境向け flowgraph を開発者の手元で検証するためのテスト。
	// CI には該当ファイルが存在しないため `#[ignore]` で既定除外し、
	// 開発者は `cargo test -- --ignored load_local_voice_to_tts` で走らせる。
	#[test]
	#[ignore]
	fn load_local_voice_to_tts_flowgraph() {
		let path = Path::new("flowgraph.local/voice-to-tts/main.flowgraph.toml");
		if !path.exists() {
			eprintln!("skipping: {} が存在しない（.local gitignore 下、開発者環境専用）", path.display());
			return;
		}
		let report = load_file(path, None).expect("voice-to-tts example should load");
		assert!(
			report.diagnostics.iter().all(|d| d.severity != Severity::Error),
			"unexpected errors: {:?}",
			report.diagnostics,
		);
	}

	#[test]
	fn parse_enums_section() {
		let src = r#"
			[[enums]]
			id = "color"
			variants = ["red", "green"]

			[[nodes]]
			id = "lit"
			feature = "flowgraph.literal.string"
		"#;
		let f = parse_flowgraph_file(src, None).unwrap();
		assert_eq!(f.enums.len(), 1);
		assert_eq!(f.enums[0].id, "color");
		assert_eq!(f.enums[0].variants, vec!["red", "green"]);
	}

	#[test]
	fn parse_groups_section() {
		let src = r##"
			[[groups]]
			id = "g1"
			label = "Input stage"
			node_ids = ["in", "log"]
			color = "#38bdf8"

			[[nodes]]
			id = "in"
			feature = "flowgraph.ingress.web_input"
		"##;
		let f = parse_flowgraph_file(src, None).unwrap();
		assert_eq!(f.groups.len(), 1);
		assert_eq!(f.groups[0].id, "g1");
		assert_eq!(f.groups[0].label.as_deref(), Some("Input stage"));
		assert_eq!(f.groups[0].node_ids, vec!["in", "log"]);
		assert_eq!(f.groups[0].color.as_deref(), Some("#38bdf8"));
	}

	#[test]
	fn load_duplicate_enum_id_errors() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[enums]]
				id = "x"
				variants = ["a"]

				[[enums]]
				id = "x"
				variants = ["b"]

				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "a"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err.errors().any(|d| d.code == DiagnosticCode::DuplicateEnumId));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_rejects_literal_tts_engine_not_in_closed_set() {
		let path = write_tmp(
			"graph.flowgraph.toml",
			r#"
				[[nodes]]
				id = "e"
				feature = "flowgraph.literal.string"
				[nodes.properties]
				value = "not_a_registered_tts_engine"

				[[nodes]]
				id = "t"
				feature = "flowgraph.tts.speak"

				[[edges]]
				from = "e:value"
				to = "t:engine"
			"#,
		);
		let err = load_file(&path, None).expect_err("should fail");
		assert!(err.errors().any(|d| d.code == DiagnosticCode::ClosedStringLiteralOutOfEnum));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn normalized_library_id_joins_meta_fields() {
		let m = FileMeta {
			author: Some(" Alice ".into()),
			name: Some("Core-Lib".into()),
			version: Some("1.0.0".into()),
			..Default::default()
		};
		assert_eq!(normalized_library_id(&m).unwrap(), "alice::core_lib::1_0_0");
	}

	#[test]
	fn load_report_includes_file_activation_meta() {
		let path = write_tmp(
			"demo.flowgraph.toml",
			r#"
				[meta]
				title = "Demo"
				mode_groups = ["assistant", "rss"]
				default_enabled = false

				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "x"
			"#,
		);
		let report = load_file(&path, Some("demo")).expect("load");
		let act = report.file_activation.get("demo").expect("fq key").clone();
		assert_eq!(act.mode_groups, vec!["assistant".to_string(), "rss".to_string()]);
		assert!(!act.default_enabled);
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_report_includes_graph_signature() {
		let path = write_tmp(
			"signature.flowgraph.toml",
			r#"
				[meta]
				title = "Signature Demo"
				author = "VAC"
				name = "demo"
				version = "1.2.3"

				[package]
				id = "example.signature"
				version = "0.2.0"
				exports = ["demo/main"]

				[[nodes]]
				id = "in"
				feature = "flowgraph.ingress.web_input"

				[[nodes]]
				id = "log"
				feature = "flowgraph.util.log"

				[[edges]]
				from = "in:exec_out"
				to = "log:exec_in"

				[[edges]]
				from = "in:content"
				to = "log:value"
			"#,
		);
		let report = load_file(&path, Some("demo/main")).expect("load");
		let sig = &report.graph_signature;

		assert_eq!(sig.version, 1);
		assert_eq!(sig.kind, "flowgraph");
		assert_eq!(sig.file_count, 1);
		assert_eq!(sig.node_count, 2);
		assert_eq!(sig.edge_count, 2);
		assert_eq!(sig.effectful_node_count, report.capability_summary.effectful_node_count);
		assert_eq!(sig.required_capabilities, report.capability_summary.capabilities);
		assert_eq!(sig.files[0].fq, "demo/main");
		assert_eq!(sig.files[0].title.as_deref(), Some("Signature Demo"));
		assert_eq!(sig.files[0].library_id.as_deref(), Some("vac::demo::1_2_3"));
		assert_eq!(sig.files[0].package_id.as_deref(), Some("example.signature"));
		assert_eq!(sig.files[0].package_version.as_deref(), Some("0.2.0"));
		assert_eq!(sig.files[0].package_exports, vec!["demo/main".to_string()]);

		let trigger = sig
			.external_triggers
			.iter()
			.find(|trigger| trigger.node == "demo/main::in")
			.expect("ingress trigger signature");
		assert_eq!(trigger.trigger_kind, "ingress");
		assert_eq!(trigger.exec_inputs, vec!["__trigger__".to_string()]);
		assert!(trigger
			.data_inputs
			.iter()
			.any(|port| port.port == "__content__" && port.ty == "string"));

		assert!(sig
			.boundary_inputs
			.iter()
			.any(|port| port.node == "demo/main::in" && port.port == "__trigger__" && port.exec));
		assert!(!sig
			.boundary_inputs
			.iter()
			.any(|port| port.node == "demo/main::log" && port.port == "value"));
		assert!(sig
			.boundary_outputs
			.iter()
			.any(|port| port.node == "demo/main::log" && port.port == "exec_out" && port.exec));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn load_rejects_empty_mode_group_name() {
		let path = write_tmp(
			"bad-meta.flowgraph.toml",
			r#"
				[meta]
				mode_groups = ["ok", ""]

				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "x"
			"#,
		);
		let err = load_file(&path, None).expect_err("empty mode group");
		assert!(err.errors().any(|d| d.code == DiagnosticCode::InvalidModeMetadata));
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
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
		for id in ["in", "cmd_path", "cmd_mode", "load_cmds", "match", "log_matched", "log_plain"] {
			let fq = format!("main::{id}");
			assert!(
				report.node_meta.contains_key(&fq),
				"node `{fq}` not resolved; got keys: {:?}",
				report.node_meta.keys().collect::<Vec<_>>(),
			);
		}
	}
}
