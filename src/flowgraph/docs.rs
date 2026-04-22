//! Node catalog（Markdown）自動生成。
//!
//! δ-8c-3 で `docs/manual/node-catalog.md` を NodeRegistry から生成するためのモジュール。
//! テスト（`docs_tests::node_catalog_md_up_to_date`）が on-disk の Markdown と一致を
//! 保証する。更新したい場合は環境変数 `BLESS_NODE_CATALOG=1` でテストを走らせると
//! 自動で書き戻す。
//!
//! ```powershell
//! $env:BLESS_NODE_CATALOG="1"; cargo test --lib node_catalog_md_up_to_date
//! ```

use crate::flowgraph::node::{NodeSpec, PortDirection, PortSpec, PropertySpec};
use crate::flowgraph::registry::NodeRegistry;
use std::collections::BTreeMap;

/// NodeRegistry の内容から `docs/manual/node-catalog.md` 相当の文字列を生成する。
///
/// - カテゴリ（`NodeSpec.category`）ごとに見出し
/// - 各 feature ごとに入力 / 出力 / プロパティの表
pub fn render_node_catalog_md(registry: &NodeRegistry) -> String {
	let mut by_category: BTreeMap<String, Vec<NodeSpec>> = BTreeMap::new();
	for spec in registry.all_specs() {
		by_category.entry(spec.category.clone()).or_default().push(spec);
	}
	for v in by_category.values_mut() {
		v.sort_by(|a, b| a.feature.cmp(&b.feature));
	}

	let mut out = String::new();
	out.push_str("# Flowgraph Node Catalog\n\n");
	out.push_str("VAC v2 Flowgraph の組み込みノード一覧。**本ファイルは自動生成**されるので、手で編集せずノード定義側を更新してからテストで再生成してください。\n\n");
	out.push_str("再生成:\n\n");
	out.push_str("```powershell\n");
	out.push_str("$env:BLESS_NODE_CATALOG=\"1\"; cargo test --lib node_catalog_md_up_to_date\n");
	out.push_str("```\n\n");
	out.push_str("> 型の表記: `bool` / `int` / `float` / `string` / `json` / `list<T>` / `map<T>` / `exec`\n\n");

	// Index
	out.push_str("## Index\n\n");
	for (category, specs) in &by_category {
		out.push_str(&format!("- **{}**\n", category));
		for spec in specs {
			out.push_str(&format!(
				"  - [`{}`](#{}) — {}\n",
				spec.feature,
				anchor(&spec.feature),
				spec.title,
			));
		}
	}
	out.push('\n');

	// Sections
	for (category, specs) in &by_category {
		out.push_str(&format!("## {}\n\n", category));
		for spec in specs {
			render_node(&mut out, spec);
		}
	}

	out
}

fn render_node(out: &mut String, spec: &NodeSpec) {
	out.push_str(&format!("### `{}`\n\n", spec.feature));
	out.push_str(&format!("**{}**", spec.title));
	if let Some(desc) = &spec.description {
		out.push_str(&format!(" — {}", desc));
	}
	out.push_str("\n\n");

	// Inputs
	let inputs: Vec<&PortSpec> = spec.inputs.iter().collect();
	if !inputs.is_empty() {
		out.push_str("| Input | Type | Default | Note |\n");
		out.push_str("|---|---|---|---|\n");
		for p in &inputs {
			out.push_str(&format!(
				"| `{}` | {} | {} | {} |\n",
				p.name,
				port_type(p),
				port_default(p),
				port_note(p),
			));
		}
		out.push('\n');
	}

	// Outputs
	let outputs: Vec<&PortSpec> = spec.outputs.iter().collect();
	if !outputs.is_empty() {
		out.push_str("| Output | Type | Note |\n");
		out.push_str("|---|---|---|\n");
		for p in &outputs {
			out.push_str(&format!(
				"| `{}` | {} | {} |\n",
				p.name,
				port_type(p),
				port_note(p),
			));
		}
		out.push('\n');
	}

	// Properties
	if !spec.properties.is_empty() {
		out.push_str("| Property | Type | Default | Required | Note |\n");
		out.push_str("|---|---|---|---|---|\n");
		for p in &spec.properties {
			out.push_str(&format!(
				"| `{}` | `{}` | `{}` | {} | {} |\n",
				p.name,
				p.ty,
				property_default(p),
				if p.required { "✔" } else { "" },
				p.description.as_deref().unwrap_or(""),
			));
		}
		out.push('\n');
	}
}

fn port_type(p: &PortSpec) -> String {
	if p.is_exec {
		match p.direction {
			PortDirection::Input => "`exec` (in)".into(),
			PortDirection::Output => "`exec` (out)".into(),
		}
	} else {
		format!("`{}`", p.ty)
	}
}

fn port_default(p: &PortSpec) -> String {
	match &p.default {
		Some(v) => format!("`{}`", compact_json(&v.0)),
		None => {
			if p.optional {
				"optional".into()
			} else {
				"—".into()
			}
		}
	}
}

fn port_note(p: &PortSpec) -> String {
	let mut parts: Vec<String> = Vec::new();
	if p.multi {
		parts.push("multi".into());
	}
	if p.optional && p.default.is_none() {
		parts.push("optional".into());
	}
	if let Some(d) = &p.description {
		parts.push(d.clone());
	}
	parts.join("; ").replace('\n', " ")
}

fn property_default(p: &PropertySpec) -> String {
	compact_json(&p.default.0)
}

fn compact_json(v: &serde_json::Value) -> String {
	serde_json::to_string(v).unwrap_or_else(|_| "?".into())
}

fn anchor(feature: &str) -> String {
	// GitHub-style anchor: lowercase, replace non-alnum with `-`, then collapse `-`.
	let mut s = String::with_capacity(feature.len());
	for c in feature.chars() {
		if c.is_ascii_alphanumeric() {
			s.push(c.to_ascii_lowercase());
		} else {
			s.push('-');
		}
	}
	while s.contains("--") {
		s = s.replace("--", "-");
	}
	s.trim_matches('-').to_string()
}

#[cfg(test)]
mod docs_tests {
	use super::*;
	use crate::flowgraph::registry::default_registry;
	use std::path::PathBuf;

	fn node_catalog_path() -> PathBuf {
		PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/manual/node-catalog.md")
	}

	#[test]
	fn node_catalog_md_up_to_date() {
		let expected = render_node_catalog_md(&default_registry());
		let path = node_catalog_path();

		if std::env::var_os("BLESS_NODE_CATALOG").is_some() {
			if let Some(parent) = path.parent() {
				std::fs::create_dir_all(parent).expect("create docs/manual");
			}
			std::fs::write(&path, &expected).expect("write node-catalog.md");
			eprintln!("[BLESS] wrote {}", path.display());
			return;
		}

		let actual = std::fs::read_to_string(&path).unwrap_or_else(|e| {
			panic!(
				"docs/manual/node-catalog.md の読み込みに失敗: {e}\n\
				 初回生成するには BLESS_NODE_CATALOG=1 を付けて再実行してください。\n\
				 （powershell: `$env:BLESS_NODE_CATALOG=\"1\"; cargo test --lib node_catalog_md_up_to_date`）"
			);
		});

		if actual != expected {
			let diff_hint = "BLESS_NODE_CATALOG=1 で再生成してください。";
			panic!("docs/manual/node-catalog.md が registry と不一致。{diff_hint}");
		}
	}
}
