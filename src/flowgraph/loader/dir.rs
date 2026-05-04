//! ディレクトリ再帰ウォーク + 複数ファイル統合 loader（spec §6.1 / §6.2 / §8）。
//!
//! - `flowgraph_root/` 以下を再帰的に走査。
//! - `*.flowgraph.toml` のみ対象。`*.flowgraph.toml.disabled` は skip。
//! - 隠しフォルダ（`.` 始まり）・アンダースコア始まりフォルダ（`_` 始まり）は skip。
//! - 全ファイルの fq path を集め、`BuildContext` を構成して単一グラフへ統合。

use crate::flowgraph::loader::diagnostic::{Diagnostic, DiagnosticCode, LoadError, LoadReport, Severity};
use crate::flowgraph::loader::file::{parse_flowgraph_file, BuildContext, FlowgraphFile};
use crate::flowgraph::registry::registry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

fn normalize_library_use_fq(s: &str) -> String {
	let mut t = s.trim().replace('\\', "/");
	while let Some(stripped) = t.strip_prefix("./") {
		t = stripped.to_string();
	}
	if let Some(stripped) = t.strip_suffix(".flowgraph.toml") {
		t = stripped.to_string();
	}
	t.trim_matches('/').to_string()
}

fn package_id_is_valid(id: &str) -> bool {
	if id != id.trim() {
		return false;
	}
	id.split('.').all(|segment| {
		let mut chars = segment.chars();
		let Some(first) = chars.next() else {
			return false;
		};
		(first.is_ascii_lowercase() || first.is_ascii_digit())
			&& chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
	})
}
fn package_version_is_valid(version: &str) -> bool {
	if version != version.trim() || version.is_empty() {
		return false;
	}
	let (without_build, build) = match version.split_once('+') {
		Some((head, tail)) => (head, Some(tail)),
		None => (version, None),
	};
	if build.is_some_and(|build| !semver_identifiers_are_valid(build, true)) {
		return false;
	}
	let (core, pre) = match without_build.split_once('-') {
		Some((head, tail)) => (head, Some(tail)),
		None => (without_build, None),
	};
	if pre.is_some_and(|pre| !semver_identifiers_are_valid(pre, false)) {
		return false;
	}
	let parts: Vec<&str> = core.split('.').collect();
	parts.len() == 3 && parts.iter().all(|part| semver_number_is_valid(part))
}

fn package_version_requirement_is_valid(requirement: &str) -> bool {
	if requirement != requirement.trim() || requirement.is_empty() {
		return false;
	}
	requirement == "*" || package_version_is_valid(requirement)
}

fn semver_number_is_valid(part: &str) -> bool {
	!part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()) && (part == "0" || !part.starts_with('0'))
}

fn semver_identifiers_are_valid(identifiers: &str, allow_numeric_leading_zero: bool) -> bool {
	!identifiers.is_empty()
		&& identifiers.split('.').all(|identifier| {
			!identifier.is_empty()
				&& identifier.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
				&& (allow_numeric_leading_zero || !identifier.chars().all(|ch| ch.is_ascii_digit()) || semver_number_is_valid(identifier))
		})
}

/// `[meta].library_uses` の参照先検証と閉路検出（エラー時はロード失敗）。
fn library_use_dependency_diagnostics(files: &[(String, PathBuf, FlowgraphFile)], known: &HashSet<String>) -> Vec<Diagnostic> {
	let mut diagnostics: Vec<Diagnostic> = Vec::new();
	let mut adj: HashMap<String, Vec<String>> = HashMap::new();

	for (fq, path, file) in files {
		let Some(meta) = file.meta.as_ref() else {
			continue;
		};
		let Some(uses) = meta.library_uses.as_ref() else {
			continue;
		};
		for raw in uses {
			let target = normalize_library_use_fq(raw);
			if target.is_empty() {
				diagnostics.push(
					Diagnostic::error(DiagnosticCode::UnknownLibraryRef, "library_uses に空エントリ")
						.with_file(path.clone())
						.with_hint(format!("[meta].library_uses / {raw:?}")),
				);
				continue;
			}
			if !known.contains(&target) {
				diagnostics.push(
					Diagnostic::error(
						DiagnosticCode::UnknownLibraryRef,
						format!("library_uses の参照先 '{target}' が flowgraph ルート内に存在しない"),
					)
					.with_file(path.clone())
					.with_hint(raw.clone()),
				);
				continue;
			}
			adj.entry(fq.clone()).or_default().push(target);
		}
	}

	if adj.is_empty() {
		return diagnostics;
	}

	let mut vertices: HashSet<String> = HashSet::new();
	for (a, bs) in &adj {
		vertices.insert(a.clone());
		for b in bs {
			vertices.insert(b.clone());
		}
	}

	let mut path_stack: Vec<String> = Vec::new();
	let mut in_stack: HashSet<String> = HashSet::new();
	let mut finished: HashSet<String> = HashSet::new();

	fn dfs_visit(
		u: &str,
		adj: &HashMap<String, Vec<String>>,
		path_stack: &mut Vec<String>,
		in_stack: &mut HashSet<String>,
		finished: &mut HashSet<String>,
	) -> Option<Vec<String>> {
		if finished.contains(u) {
			return None;
		}
		if in_stack.contains(u) {
			let idx = path_stack.iter().position(|x| x == u)?;
			let mut cyc = path_stack[idx..].to_vec();
			cyc.push(u.to_string());
			return Some(cyc);
		}
		in_stack.insert(u.to_string());
		path_stack.push(u.to_string());
		for v in adj.get(u).into_iter().flatten() {
			if let Some(c) = dfs_visit(v, adj, path_stack, in_stack, finished) {
				return Some(c);
			}
		}
		path_stack.pop();
		in_stack.remove(u);
		finished.insert(u.to_string());
		None
	}

	for v in vertices {
		if finished.contains(&v) {
			continue;
		}
		path_stack.clear();
		in_stack.clear();
		if let Some(cyc) = dfs_visit(&v, &adj, &mut path_stack, &mut in_stack, &mut finished) {
			diagnostics.push(Diagnostic::error(
				DiagnosticCode::LibraryDependencyCycle,
				format!("library_uses に閉路: {}", cyc.join(" -> ")),
			));
			break;
		}
	}

	diagnostics
}

fn package_manifest_diagnostics(files: &[(String, PathBuf, FlowgraphFile)], known: &HashSet<String>) -> Vec<Diagnostic> {
	let mut diagnostics: Vec<Diagnostic> = Vec::new();
	let mut seen_package_ids: HashMap<String, PathBuf> = HashMap::new();
	for (_fq, path, file) in files {
		let Some(package) = file.package.as_ref() else {
			continue;
		};

		match package.id.as_deref() {
			None | Some("") => diagnostics.push(
				Diagnostic::error(DiagnosticCode::InvalidPackageManifest, "[package].id が未指定または空です")
					.with_file(path.clone())
					.with_hint("[package].id"),
			),
			Some(id) if !package_id_is_valid(id) => diagnostics.push(
				Diagnostic::error(
					DiagnosticCode::InvalidPackageManifest,
					format!("[package].id '{id}' は lowercase dot-separated identifier ではありません"),
				)
				.with_file(path.clone())
				.with_hint("[package].id"),
			),
			Some(id) => {
				if let Some(first_path) = seen_package_ids.insert(id.to_string(), path.clone()) {
					diagnostics.push(
						Diagnostic::error(
							DiagnosticCode::InvalidPackageManifest,
							format!("[package].id が重複しています: '{id}'"),
						)
						.with_file(path.clone())
						.with_hint(format!("first declared in {}", first_path.display())),
					);
				}
			}
		}

		if let Some(version) = package.version.as_deref() {
			if !package_version_is_valid(version) {
				diagnostics.push(
					Diagnostic::error(
						DiagnosticCode::InvalidPackageManifest,
						format!("[package].version '{version}' は SemVer ではありません"),
					)
					.with_file(path.clone())
					.with_hint("[package].version"),
				);
			}
		}

		for (dependency_id, requirement) in &package.dependencies {
			if !package_id_is_valid(dependency_id) {
				diagnostics.push(
					Diagnostic::error(
						DiagnosticCode::InvalidPackageManifest,
						format!("[package].dependencies の id '{dependency_id}' は lowercase dot-separated identifier ではありません"),
					)
					.with_file(path.clone())
					.with_hint(format!("[package].dependencies.{dependency_id}")),
				);
			}
			if !package_version_requirement_is_valid(requirement) {
				diagnostics.push(
					Diagnostic::error(
						DiagnosticCode::InvalidPackageManifest,
						format!("[package].dependencies.{dependency_id} = '{requirement}' は未対応の version requirement です"),
					)
					.with_file(path.clone())
					.with_hint(format!("[package].dependencies.{dependency_id}")),
				);
			}
			if package.id.as_deref() == Some(dependency_id.as_str()) {
				diagnostics.push(
					Diagnostic::error(
						DiagnosticCode::InvalidPackageManifest,
						format!("[package].dependencies が自分自身を参照しています: '{dependency_id}'"),
					)
					.with_file(path.clone())
					.with_hint(format!("[package].dependencies.{dependency_id}")),
				);
			}
		}

		let mut seen_exports: HashSet<String> = HashSet::new();
		for export in &package.exports {
			let target = normalize_library_use_fq(export);
			if target.is_empty() {
				diagnostics.push(
					Diagnostic::error(DiagnosticCode::InvalidPackageManifest, "[package].exports に空エントリ")
						.with_file(path.clone())
						.with_hint(format!("[package].exports / {export:?}")),
				);
				continue;
			}
			if !seen_exports.insert(target.clone()) {
				diagnostics.push(
					Diagnostic::error(
						DiagnosticCode::InvalidPackageManifest,
						format!("[package].exports に重複: '{target}'"),
					)
					.with_file(path.clone())
					.with_hint(export.clone()),
				);
				continue;
			}
			if !known.contains(&target) {
				diagnostics.push(
					Diagnostic::error(
						DiagnosticCode::InvalidPackageManifest,
						format!("[package].exports の参照先 '{target}' が flowgraph ルート内に存在しない"),
					)
					.with_file(path.clone())
					.with_hint(export.clone()),
				);
			}
		}
	}
	diagnostics
}

/// 指定パスが `*.flowgraph.toml`（`.disabled` は除外）か。
pub fn is_flowgraph_file(p: &Path) -> bool {
	let name = match p.file_name().and_then(|s| s.to_str()) {
		Some(s) => s,
		None => return false,
	};
	if !name.ends_with(".flowgraph.toml") {
		return false;
	}
	if name.ends_with(".disabled") {
		return false;
	}
	true
}

/// `flowgraph_root` を再帰ウォークして `.flowgraph.toml` ファイル一覧を返す。
///
/// 走査結果はファイルパスのソート順（OS 非依存の決定論のため）。
pub fn walk_flowgraph_dir(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
	let mut out: Vec<PathBuf> = Vec::new();
	let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
	while let Some(dir) = stack.pop() {
		let entries = match std::fs::read_dir(&dir) {
			Ok(v) => v,
			Err(e) => {
				if dir == root {
					return Err(e);
				}
				// サブディレクトリへのアクセス失敗は skip
				continue;
			}
		};
		for entry in entries.flatten() {
			let p = entry.path();
			let name = match p.file_name().and_then(|s| s.to_str()) {
				Some(s) => s.to_string(),
				None => continue,
			};
			let is_dir = match entry.file_type() {
				Ok(ft) => ft.is_dir(),
				Err(_) => continue,
			};
			if is_dir {
				if name.starts_with('.') || name.starts_with('_') {
					continue;
				}
				stack.push(p);
			} else if is_flowgraph_file(&p) {
				out.push(p);
			}
		}
	}
	out.sort();
	Ok(out)
}

/// 絶対ファイルパスを fq path（拡張子なし root 相対、`/` 区切り）に変換。
///
/// - `<root>/tts/jp_routing.flowgraph.toml` → `tts/jp_routing`
/// - `<root>/main.flowgraph.toml` → `main`
/// - `<root>/tts/main.flowgraph.toml` → `tts/main`（main 規約解決は loader 側で吸収）
pub fn fq_path_of_file(root: &Path, file: &Path) -> Result<String, String> {
	let rel = file
		.strip_prefix(root)
		.map_err(|_| format!("ファイル '{}' が root '{}' 外", file.display(), root.display()))?;
	let s = rel.to_string_lossy().replace('\\', "/");
	let stripped = s
		.strip_suffix(".flowgraph.toml")
		.ok_or_else(|| format!("'.flowgraph.toml' で終わらない: '{s}'"))?;
	Ok(stripped.to_string())
}

/// `flowgraph_root` を走査し、全ファイルを統合した `FlowgraphProgram` を返す。
pub fn load_flowgraph_dir(root: &Path) -> Result<LoadReport, LoadError> {
	if !root.exists() {
		return Err(LoadError::from_single(
			Diagnostic::error(DiagnosticCode::Io, format!("flowgraph root が存在しない: '{}'", root.display()))
				.with_file(root.to_path_buf()),
		));
	}

	let files = walk_flowgraph_dir(root).map_err(|e| {
		LoadError::from_single(Diagnostic::error(DiagnosticCode::Io, format!("ディレクトリ走査失敗: {e}")).with_file(root.to_path_buf()))
	})?;

	if files.is_empty() {
		// 空は warning: プログラムは空グラフで構築可能にしておく
		let ctx = BuildContext {
			files: vec![],
			known_file_fqs: HashSet::new(),
		};
		return ctx.build(registry());
	}

	let mut parsed: Vec<(String, PathBuf, FlowgraphFile)> = Vec::new();
	let mut known_file_fqs: HashSet<String> = HashSet::new();
	let mut parse_diags: Vec<Diagnostic> = Vec::new();

	for file_path in files {
		let fq = match fq_path_of_file(root, &file_path) {
			Ok(v) => v,
			Err(e) => {
				parse_diags.push(Diagnostic::error(DiagnosticCode::Io, e).with_file(file_path.clone()));
				continue;
			}
		};

		let src = match std::fs::read_to_string(&file_path) {
			Ok(v) => v,
			Err(e) => {
				parse_diags.push(Diagnostic::error(DiagnosticCode::Io, format!("読み込み失敗: {e}")).with_file(file_path.clone()));
				continue;
			}
		};

		match parse_flowgraph_file(&src, Some(&file_path)) {
			Ok(file) => {
				if !known_file_fqs.insert(fq.clone()) {
					parse_diags.push(
						Diagnostic::error(DiagnosticCode::DuplicateNodeId, format!("fq path 重複: '{fq}'")).with_file(file_path.clone()),
					);
					continue;
				}
				parsed.push((fq, file_path, file));
			}
			Err(mut e) => parse_diags.append(&mut e.diagnostics),
		}
	}

	if parse_diags.iter().any(|d| d.severity == Severity::Error) {
		return Err(LoadError::new(parse_diags));
	}

	let mut lib_diags = library_use_dependency_diagnostics(&parsed, &known_file_fqs);
	if lib_diags.iter().any(|d| d.severity == Severity::Error) {
		parse_diags.append(&mut lib_diags);
		return Err(LoadError::new(parse_diags));
	}

	let mut package_diags = package_manifest_diagnostics(&parsed, &known_file_fqs);
	if package_diags.iter().any(|d| d.severity == Severity::Error) {
		parse_diags.append(&mut package_diags);
		return Err(LoadError::new(parse_diags));
	}

	let ctx = BuildContext {
		files: parsed,
		known_file_fqs,
	};
	let mut report = ctx.build(registry())?;
	// パース時の warning を合流
	let mut all = parse_diags;
	all.append(&mut lib_diags);
	all.append(&mut package_diags);
	all.append(&mut report.diagnostics);
	report.diagnostics = all;
	Ok(report)
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::loader::{DiagnosticCode, Severity};

	fn tmp_root() -> PathBuf {
		use std::time::{SystemTime, UNIX_EPOCH};
		let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
		let dir = std::env::temp_dir().join(format!("vac-fg-dir-{}-{ns}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	fn write(p: &Path, s: &str) {
		if let Some(parent) = p.parent() {
			std::fs::create_dir_all(parent).unwrap();
		}
		std::fs::write(p, s).unwrap();
	}

	#[test]
	fn walk_includes_only_flowgraph_toml() {
		let root = tmp_root();
		write(&root.join("a.flowgraph.toml"), "");
		write(&root.join("README.md"), "not flowgraph");
		write(&root.join("b.flowgraph.toml.disabled"), "skipped");
		write(&root.join("sub/c.flowgraph.toml"), "");
		write(&root.join(".hidden/d.flowgraph.toml"), "");
		write(&root.join("_scratch/e.flowgraph.toml"), "");

		let files = walk_flowgraph_dir(&root).unwrap();
		let names: Vec<String> = files
			.iter()
			.map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))
			.collect();
		assert_eq!(names, vec!["a.flowgraph.toml", "sub/c.flowgraph.toml"]);

		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn fq_path_conversion() {
		let root = Path::new("/root");
		assert_eq!(fq_path_of_file(root, Path::new("/root/main.flowgraph.toml")).unwrap(), "main");
		assert_eq!(fq_path_of_file(root, Path::new("/root/tts/jp.flowgraph.toml")).unwrap(), "tts/jp");
		assert_eq!(
			fq_path_of_file(root, Path::new("/root/tts/main.flowgraph.toml")).unwrap(),
			"tts/main"
		);
		assert!(fq_path_of_file(root, Path::new("/other/a.flowgraph.toml")).is_err());
	}

	#[test]
	fn load_dir_with_cross_file_edges() {
		let root = tmp_root();
		write(
			&root.join("producer.flowgraph.toml"),
			r#"
				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "hi"
			"#,
		);
		write(
			&root.join("consumer.flowgraph.toml"),
			r#"
				[[nodes]]
				id = "logger"
				feature = "flowgraph.util.log"

				[[edges]]
				from = "/producer::lit:value"
				to = "logger:value"
			"#,
		);

		let report = load_flowgraph_dir(&root).expect("should load");
		assert!(report.node_meta.contains_key("producer::lit"));
		assert!(report.node_meta.contains_key("consumer::logger"));
		// no errors
		assert!(report.diagnostics.iter().all(|d| d.severity != Severity::Error));
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn load_dir_main_rule_resolution() {
		let root = tmp_root();
		write(
			&root.join("tts/main.flowgraph.toml"),
			r#"
				[[nodes]]
				id = "lit"
				feature = "flowgraph.literal.string"
				properties.value = "hi"
			"#,
		);
		write(
			&root.join("consumer.flowgraph.toml"),
			r#"
				[[nodes]]
				id = "logger"
				feature = "flowgraph.util.log"

				[[edges]]
				from = "tts::lit:value"
				to = "logger:value"
			"#,
		);

		let report = load_flowgraph_dir(&root).expect("should load via main rule");
		assert!(report.node_meta.contains_key("tts/main::lit"));
		assert!(report.node_meta.contains_key("consumer::logger"));
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn load_dir_relative_path() {
		let root = tmp_root();
		write(
			&root.join("ingress/twitch.flowgraph.toml"),
			r#"
				[[nodes]]
				id = "in"
				feature = "flowgraph.literal.string"
				properties.value = "hi"
			"#,
		);
		write(
			&root.join("ingress/route.flowgraph.toml"),
			r#"
				[[nodes]]
				id = "logger"
				feature = "flowgraph.util.log"

				[[edges]]
				from = "./twitch::in:value"
				to = "logger:value"
			"#,
		);

		let report = load_flowgraph_dir(&root).expect("should load");
		assert!(report.node_meta.contains_key("ingress/twitch::in"));
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn load_dir_empty_is_ok() {
		let root = tmp_root();
		// No files at all: loader should return an empty program without errors.
		let report = load_flowgraph_dir(&root).expect("empty dir should be valid");
		assert!(report.diagnostics.iter().all(|d| d.severity != Severity::Error));
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn load_dir_nonexistent_root_errors() {
		let err = load_flowgraph_dir(Path::new("c:/vac-nonexistent-xyz-12345")).unwrap_err();
		assert!(err.errors().any(|d| d.code == DiagnosticCode::Io));
	}

	/// リポジトリ同梱の `flowgraph.example/` をそのままロードできることを保証する。
	/// 例ファイルが「常に合法な flowgraph」であることを保つ回帰テスト。
	#[test]
	fn examples_dir_loads_without_errors() {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("flowgraph.example");
		let report =
			load_flowgraph_dir(&root).unwrap_or_else(|e| panic!("flowgraph.example の load に失敗: diagnostics={:#?}", e.diagnostics));
		// エラーが無い
		assert!(
			report.diagnostics.iter().all(|d| d.severity != Severity::Error),
			"flowgraph.example に error 診断が含まれる: {:#?}",
			report.diagnostics
		);
		// ルートの hello サンプル
		assert!(report.node_meta.contains_key("main::hello"));
		assert!(report.node_meta.contains_key("main::logger"));
		// multi-file サンプル
		assert!(report.node_meta.contains_key("chat-echo/main::in"));
		assert!(report.node_meta.contains_key("chat-echo/tts::speaker"));
		assert_eq!(report.capability_summary.node_count, report.node_meta.len());
		assert!(report.capability_summary.effectful_node_count > 0);
		assert!(report.capability_summary.stateful_node_count > 0);
		assert_eq!(
			report.capability_summary.stateful_node_count,
			report.capability_summary.state_nodes.len()
		);
		assert_eq!(
			report.capability_summary.volatile_state_node_count,
			report.capability_summary.state_nodes.len()
		);
		assert_eq!(report.capability_summary.snapshot_supported_state_node_count, 6);
		assert_eq!(report.capability_summary.restore_supported_state_node_count, 6);
		assert!(report.capability_summary.state_nodes.iter().any(|node| {
			node.feature == "flowgraph.util.rate_limit"
				&& node.scope == crate::flowgraph::StateScope::NodeInstance
				&& node.storage == crate::flowgraph::StateStorage::Volatile
				&& node.snapshot_policy == crate::flowgraph::StateSnapshotPolicy::Explicit
				&& node.snapshot_format == crate::flowgraph::StateSnapshotFormat::Json
				&& node.restore_supported
				&& node.restore_policy == crate::flowgraph::StateRestorePolicy::Explicit
				&& node.migration_policy == crate::flowgraph::StateMigrationPolicy::None
				&& node.persistence_policy == crate::flowgraph::StatePersistencePolicy::None
		}));
		for cap in ["network", "file_read", "file_write", "trace_write"] {
			assert!(
				report.capability_summary.capabilities.iter().any(|actual| actual == cap),
				"capability_summary に {cap} が含まれていない: {:?}",
				report.capability_summary
			);
		}
		let counts = report.capability_summary.counts_by_capability();
		assert!(counts.get("trace_write").copied().unwrap_or_default() > 0);
	}

	#[test]
	fn library_uses_cycle_returns_error() {
		let root = tmp_root();
		write(
			&root.join("a.flowgraph.toml"),
			r#"[meta]
library_uses = ["b"]

"#,
		);
		write(
			&root.join("b.flowgraph.toml"),
			r#"[meta]
library_uses = ["a"]

"#,
		);
		let err = load_flowgraph_dir(&root).expect_err("cycle");
		assert!(
			err.errors().any(|d| d.code == DiagnosticCode::LibraryDependencyCycle),
			"{:#?}",
			err.diagnostics
		);
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn package_exports_unknown_fq_returns_error() {
		let root = tmp_root();
		write(
			&root.join("main.flowgraph.toml"),
			r#"[package]
id = "example.bad"
version = "0.1.0"
exports = ["missing"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"

"#,
		);
		let err = load_flowgraph_dir(&root).expect_err("unknown package export");
		assert!(
			err.errors().any(|d| d.code == DiagnosticCode::InvalidPackageManifest),
			"{:#?}",
			err.diagnostics
		);
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn package_missing_id_returns_error() {
		let root = tmp_root();
		write(
			&root.join("main.flowgraph.toml"),
			r#"[package]
exports = ["main"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"

"#,
		);
		let err = load_flowgraph_dir(&root).expect_err("missing package id");
		assert!(
			err.errors().any(|d| d.code == DiagnosticCode::InvalidPackageManifest),
			"{:#?}",
			err.diagnostics
		);
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn package_duplicate_exports_return_error() {
		let root = tmp_root();
		write(
			&root.join("main.flowgraph.toml"),
			r#"[package]
id = "example.duplicate"
exports = ["main", "./main.flowgraph.toml"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"

"#,
		);
		let err = load_flowgraph_dir(&root).expect_err("duplicate package exports");
		assert!(
			err.errors().any(|d| d.code == DiagnosticCode::InvalidPackageManifest),
			"{:#?}",
			err.diagnostics
		);
		let _ = std::fs::remove_dir_all(&root);
	}

	#[test]
	fn package_exports_existing_fq_loads() {
		let root = tmp_root();
		write(
			&root.join("main.flowgraph.toml"),
			r#"[package]
id = "example.good"
version = "0.1.0"
exports = ["main"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"

"#,
		);
		let report = load_flowgraph_dir(&root).expect("known package export");
		assert_eq!(report.graph_signature.files[0].package_id.as_deref(), Some("example.good"));
		assert_eq!(report.graph_signature.files[0].package_exports, vec!["main".to_string()]);
		let _ = std::fs::remove_dir_all(&root);
	}

	/// 開発者用の `flowgraph.local/`（conf.local.*.toml が `flowgraph_dir` で指す）を
	/// クリーンチェックアウトでも試せる回帰テスト。
	/// CI やクリーンチェックアウトでは存在しないので、ファイルが無ければ silently skip する。
	#[test]
	fn local_dir_loads_without_errors_when_present() {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("flowgraph.local");
		if !root.is_dir() {
			return;
		}
		let report =
			load_flowgraph_dir(&root).unwrap_or_else(|e| panic!("flowgraph.local の load に失敗: diagnostics={:#?}", e.diagnostics));
		let errors: Vec<_> = report.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
		assert!(errors.is_empty(), "flowgraph.local に error 診断が含まれる: {:#?}", errors);
	}
}
