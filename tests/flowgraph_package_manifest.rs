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

	let report = load_flowgraph_dir(&root).expect("valid package version");
	assert_eq!(
		report.graph_signature.files[0].package_version.as_deref(),
		Some("1.2.3-alpha.1+build.5")
	);
	assert_eq!(report.package_manifests.len(), 1);
	assert_eq!(report.package_manifests[0].source_fq, "main");
	assert_eq!(report.package_manifests[0].id.as_deref(), Some("example.version"));
	assert_eq!(report.package_manifests[0].version.as_deref(), Some("1.2.3-alpha.1+build.5"));
	assert_eq!(report.package_manifests[0].exports, vec!["main".to_string()]);
	assert_eq!(
		report.package_manifests[0].dependencies.get("example.dep").map(String::as_str),
		Some("2.0.0")
	);
	assert_eq!(
		report.package_manifests[0].dependencies.get("example.tooling").map(String::as_str),
		Some("*")
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
