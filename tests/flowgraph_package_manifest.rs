use std::path::{Path, PathBuf};

use virtual_avatar_connect::flowgraph::loader::{load_flowgraph_dir, DiagnosticCode};

fn tmp_root(name: &str) -> PathBuf {
	use std::time::{SystemTime, UNIX_EPOCH};
	let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
	let root = std::env::temp_dir().join(format!("vac-flowgraph-package-manifest-{name}-{}-{ns}", std::process::id()));
	std::fs::create_dir_all(&root).unwrap();
	root
}

fn write(path: &Path, contents: &str) {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent).unwrap();
	}
	std::fs::write(path, contents).unwrap();
}

#[test]
fn package_invalid_id_returns_error() {
	let root = tmp_root("invalid-id");
	write(
		&root.join("main.flowgraph.toml"),
		r#"[package]
id = "Example/Bad"
exports = ["main"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("invalid package id");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_duplicate_id_returns_error() {
	let root = tmp_root("duplicate-id");
	write(
		&root.join("main.flowgraph.toml"),
		r#"[package]
id = "example.same"
exports = ["main"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"
"#,
	);
	write(
		&root.join("other.flowgraph.toml"),
		r#"[package]
id = "example.same"
exports = ["other"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "bye"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("duplicate package id");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_invalid_version_returns_error() {
	let root = tmp_root("invalid-version");
	write(
		&root.join("main.flowgraph.toml"),
		r#"[package]
id = "example.version"
version = "1.02.0"
exports = ["main"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("invalid package version");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_semver_prerelease_and_build_loads() {
	let root = tmp_root("valid-version");
	write(
		&root.join("dep.flowgraph.toml"),
		r#"[package]
id = "example.dep"
version = "2.0.0"
exports = ["dep"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "dep"
"#,
	);
	write(
		&root.join("main.flowgraph.toml"),
		r#"[package]
id = "example.version"
version = "1.2.3-alpha.1+build.5"
exports = ["main"]

[package.dependencies]
"example.dep" = "2.0.0"
"example.tooling" = "*"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"
"#,
	);
	write(
		&root.join("tooling.flowgraph.toml"),
		r#"[package]
id = "example.tooling"
exports = ["tooling"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "tooling"
"#,
	);

	let report = load_flowgraph_dir(&root).expect("valid package version");
	let main_signature = report
		.graph_signature
		.files
		.iter()
		.find(|file| file.fq == "main")
		.expect("main signature file");
	assert_eq!(main_signature.package_version.as_deref(), Some("1.2.3-alpha.1+build.5"));
	assert_eq!(report.package_manifests.len(), 3);
	let main_manifest = report
		.package_manifests
		.iter()
		.find(|manifest| manifest.id.as_deref() == Some("example.version"))
		.expect("main package manifest");
	assert_eq!(main_manifest.source_fq, "main");
	assert_eq!(main_manifest.version.as_deref(), Some("1.2.3-alpha.1+build.5"));
	assert_eq!(main_manifest.exports, vec!["main".to_string()]);
	assert_eq!(main_manifest.dependencies.get("example.dep").map(String::as_str), Some("2.0.0"));
	assert_eq!(main_manifest.dependencies.get("example.tooling").map(String::as_str), Some("*"));
	assert_eq!(
		report.package_dependency_order,
		vec![
			"example.dep".to_string(),
			"example.tooling".to_string(),
			"example.version".to_string()
		]
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_invalid_dependency_returns_error() {
	let root = tmp_root("invalid-dependency");
	write(
		&root.join("main.flowgraph.toml"),
		r#"[package]
id = "example.main"
exports = ["main"]

[package.dependencies]
"Example.Bad" = "1.0.0"
"example.main" = "*"
"example.other" = "1.02.0"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("invalid package dependency");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_missing_dependency_returns_error() {
	let root = tmp_root("missing-dependency");
	write(
		&root.join("main.flowgraph.toml"),
		r#"[package]
id = "example.main"
exports = ["main"]

[package.dependencies]
"example.missing" = "*"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("missing package dependency");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_dependency_version_mismatch_returns_error() {
	let root = tmp_root("dependency-version-mismatch");
	write(
		&root.join("dep.flowgraph.toml"),
		r#"[package]
id = "example.dep"
version = "2.0.0"
exports = ["dep"]

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "dep"
"#,
	);
	write(
		&root.join("main.flowgraph.toml"),
		r#"[package]
id = "example.main"
exports = ["main"]

[package.dependencies]
"example.dep" = "1.0.0"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "hi"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("dependency version mismatch");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_dependency_version_mismatch_is_not_cycle_edge() {
	let root = tmp_root("dependency-version-mismatch-not-cycle-edge");
	write(
		&root.join("a.flowgraph.toml"),
		r#"[package]
id = "example.a"
version = "1.0.0"
exports = ["a"]

[package.dependencies]
"example.b" = "2.0.0"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "a"
"#,
	);
	write(
		&root.join("b.flowgraph.toml"),
		r#"[package]
id = "example.b"
version = "1.0.0"
exports = ["b"]

[package.dependencies]
"example.a" = "*"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "b"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("dependency version mismatch");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	assert!(
		!err.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("閉路")),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn package_dependency_cycle_returns_error() {
	let root = tmp_root("dependency-cycle");
	write(
		&root.join("a.flowgraph.toml"),
		r#"[package]
id = "example.a"
exports = ["a"]

[package.dependencies]
"example.b" = "*"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "a"
"#,
	);
	write(
		&root.join("b.flowgraph.toml"),
		r#"[package]
id = "example.b"
exports = ["b"]

[package.dependencies]
"example.c" = "*"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "b"
"#,
	);
	write(
		&root.join("c.flowgraph.toml"),
		r#"[package]
id = "example.c"
exports = ["c"]

[package.dependencies]
"example.a" = "*"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
properties.value = "c"
"#,
	);

	let err = load_flowgraph_dir(&root).expect_err("package dependency cycle");
	assert!(
		err.errors()
			.any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidPackageManifest),
		"{:#?}",
		err.diagnostics
	);
	let _ = std::fs::remove_dir_all(&root);
}
