//! Node catalog（Markdown）自動生成。
//!
//! δ-8c-3 で `docs/manual/node-catalog.md` を NodeRegistry から生成するためのモジュール。
//! テスト（`docs_tests::node_catalog_md_up_to_date`）が on-disk の Markdown と一致を
//! 保証する。更新したい場合は repository-local script でテストを走らせると自動で書き戻す。
//!
//! ```powershell
//! ./scripts/bless-node-catalog.ps1
//! # or:
//! $env:BLESS_NODE_CATALOG="1"; cargo test --lib node_catalog_md_up_to_date
//! ```

use crate::flowgraph::node::{NodeSpec, PortDirection, PortSpec, PropertySpec};
use crate::flowgraph::registry::NodeRegistry;
use crate::flowgraph::FlowgraphStateModel;
use serde::Serialize;
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
	out.push_str("./scripts/bless-node-catalog.ps1\n");
	out.push_str("# or:\n");
	out.push_str("$env:BLESS_NODE_CATALOG=\"1\"; cargo test --lib node_catalog_md_up_to_date\n");
	out.push_str("```\n\n");
	out.push_str("> 型の表記: `bool` / `int` / `float` / `string` / `bytes` / `json` / `list<T>` / `map<T>` / `exec`\n\n");
	out.push_str("## Reading This Catalog\n\n");
	out.push_str("- **Index**: category ごとの通常一覧。feature 名から node 詳細へ移動するための入口です。\n");
	out.push_str("- **Metadata Index**: effect / capability / snapshot support から node を逆引きするための一覧です。GUI catalog と同じ registry metadata から生成します。\n");
	out.push_str(
		"- 各 node section の **Metadata** line は contract summary、effect class、capability、state model を compact に示します。\n\n",
	);
	render_catalog_summary(&mut out, registry, &by_category);

	// Index
	out.push_str("## Index\n\n");
	for (category, specs) in &by_category {
		out.push_str(&format!("- **{}**\n", category));
		for spec in specs {
			out.push_str(&format!("  - [`{}`](#{}) — {}\n", spec.feature, anchor(&spec.feature), spec.title,));
		}
	}
	out.push('\n');
	render_metadata_index(&mut out, registry, &by_category);

	// Sections
	for (category, specs) in &by_category {
		out.push_str(&format!("## {}\n\n", category));
		for spec in specs {
			render_node(&mut out, registry, spec);
		}
	}

	out
}

fn render_catalog_summary(out: &mut String, registry: &NodeRegistry, by_category: &BTreeMap<String, Vec<NodeSpec>>) {
	let specs: Vec<&NodeSpec> = by_category.values().flat_map(|items| items.iter()).collect();
	let total_nodes = specs.len();
	let effectful_count = specs
		.iter()
		.filter(|spec| registry.effect_class(&spec.feature) == Some("effectful"))
		.count();
	let stateful_count = specs
		.iter()
		.filter(|spec| registry.effect_class(&spec.feature) == Some("stateful"))
		.count();
	let capability_group_count = specs
		.iter()
		.flat_map(|spec| registry.capabilities(&spec.feature))
		.collect::<std::collections::BTreeSet<_>>()
		.len();
	let snapshot_supported_count = specs
		.iter()
		.filter(|spec| {
			registry
				.state_model(&spec.feature)
				.map(|model| model.snapshot_supported)
				.unwrap_or(false)
		})
		.count();
	let restore_supported_count = specs
		.iter()
		.filter(|spec| {
			registry
				.state_model(&spec.feature)
				.map(|model| model.restore_supported)
				.unwrap_or(false)
		})
		.count();

	out.push_str("## Generated Summary\n\n");
	out.push_str("| Metric | Count |\n");
	out.push_str("|---|---:|\n");
	out.push_str(&format!("| Nodes | {} |\n", total_nodes));
	out.push_str(&format!("| Categories | {} |\n", by_category.len()));
	out.push_str(&format!("| Effectful nodes | {} |\n", effectful_count));
	out.push_str(&format!("| Stateful nodes | {} |\n", stateful_count));
	out.push_str(&format!("| Capability groups | {} |\n", capability_group_count));
	out.push_str(&format!("| Snapshot-supported nodes | {} |\n", snapshot_supported_count));
	out.push_str(&format!("| Restore-supported nodes | {} |\n\n", restore_supported_count));
}

fn render_metadata_index(out: &mut String, registry: &NodeRegistry, by_category: &BTreeMap<String, Vec<NodeSpec>>) {
	let specs: Vec<&NodeSpec> = by_category.values().flat_map(|items| items.iter()).collect();
	let mut effectful = Vec::new();
	let mut stateful = Vec::new();
	let mut by_capability: BTreeMap<String, Vec<&NodeSpec>> = BTreeMap::new();
	let mut snapshot_supported = Vec::new();
	let mut restore_supported = Vec::new();

	for spec in specs {
		match registry.effect_class(&spec.feature).unwrap_or("unknown") {
			"effectful" => effectful.push(spec),
			"stateful" => stateful.push(spec),
			_ => {}
		}
		for cap in registry.capabilities(&spec.feature) {
			by_capability.entry(cap.to_string()).or_default().push(spec);
		}
		if let Some(model) = registry.state_model(&spec.feature) {
			if model.snapshot_supported {
				snapshot_supported.push(spec);
			}
			if model.restore_supported {
				restore_supported.push(spec);
			}
		}
	}

	out.push_str("## Metadata Index\n\n");
	out.push_str("### Effect Classes\n\n");
	out.push_str(&format!("- **effectful** ({}): {}\n", effectful.len(), feature_links(&effectful)));
	out.push_str(&format!("- **stateful** ({}): {}\n\n", stateful.len(), feature_links(&stateful)));
	out.push_str("### Capability Groups\n\n");
	if by_capability.is_empty() {
		out.push_str("- —\n\n");
	} else {
		for (capability, specs) in by_capability {
			out.push_str(&format!("- **{}** ({}): {}\n", capability, specs.len(), feature_links(&specs)));
		}
		out.push('\n');
	}
	out.push_str("### Snapshot / Restore Support\n\n");
	out.push_str(&format!(
		"- **snapshot** ({}): {}\n",
		snapshot_supported.len(),
		feature_links(&snapshot_supported),
	));
	out.push_str(&format!(
		"- **restore** ({}): {}\n\n",
		restore_supported.len(),
		feature_links(&restore_supported),
	));
}

fn feature_links(specs: &[&NodeSpec]) -> String {
	if specs.is_empty() {
		return "—".into();
	}
	specs
		.iter()
		.map(|spec| format!("[`{}`](#{})", spec.feature, anchor(&spec.feature)))
		.collect::<Vec<_>>()
		.join(", ")
}

fn render_node(out: &mut String, registry: &NodeRegistry, spec: &NodeSpec) {
	out.push_str(&format!("### `{}`\n\n", spec.feature));
	out.push_str(&format!("**{}**", spec.title));
	if let Some(desc) = &spec.description {
		out.push_str(&format!(" — {}", desc));
	}
	out.push_str("\n");
	render_node_metadata(out, registry, spec);

	// Inputs
	let inputs: Vec<&PortSpec> = spec.inputs.iter().collect();
	if !inputs.is_empty() {
		out.push_str("| Input | Type | Default | Note |\n");
		out.push_str("|---|---|---|---|\n");
		for p in &inputs {
			out.push_str(&format!(
				"| `{}` | {} | {} | {} |\n",
				table_cell(&p.name),
				table_cell(&port_type(p)),
				table_cell(&port_default(p)),
				table_cell(&port_note(p)),
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
				table_cell(&p.name),
				table_cell(&port_type(p)),
				table_cell(&port_note(p)),
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
				table_cell(&p.name),
				table_cell(&p.ty.to_string()),
				table_cell(&property_default(p)),
				if p.required { "✔" } else { "" },
				table_cell(p.description.as_deref().unwrap_or("")),
			));
		}
		out.push('\n');
	}
}

fn render_node_metadata(out: &mut String, registry: &NodeRegistry, spec: &NodeSpec) {
	let effect_class = registry.effect_class(&spec.feature).unwrap_or("unknown");
	let capabilities = registry.capabilities(&spec.feature);
	let state_model = registry
		.state_model(&spec.feature)
		.unwrap_or_else(|| FlowgraphStateModel::for_effect_class(effect_class));

	out.push_str(&format!(
		"\n**Metadata:** contract: {}; effect: `{}`; capabilities: {}; state: {}\n\n",
		contract_summary(spec),
		effect_class,
		capability_summary(&capabilities),
		state_summary(&state_model),
	));
}

fn contract_summary(spec: &NodeSpec) -> String {
	let has_exec_input = spec.inputs.iter().any(|p| p.is_exec);
	let has_exec_output = spec.outputs.iter().any(|p| p.is_exec);
	format!(
		"inputs={} / outputs={} / properties={} / exec_in={} / exec_out={}",
		spec.inputs.len(),
		spec.outputs.len(),
		spec.properties.len(),
		if has_exec_input { "yes" } else { "no" },
		if has_exec_output { "yes" } else { "no" },
	)
}

fn capability_summary(capabilities: &[&str]) -> String {
	if capabilities.is_empty() {
		"—".into()
	} else {
		capabilities.iter().map(|cap| format!("`{}`", cap)).collect::<Vec<_>>().join(", ")
	}
}

fn state_summary(model: &FlowgraphStateModel) -> String {
	if !model.stateful {
		return "stateless".into();
	}
	let snapshot = if model.snapshot_supported {
		format!("{}/{}", json_label(&model.snapshot_policy), json_label(&model.snapshot_format))
	} else {
		"unsupported".into()
	};
	let restore = if model.restore_supported {
		json_label(&model.restore_policy)
	} else {
		"unsupported".into()
	};
	format!(
		"stateful; scope=`{}`; storage=`{}`; lifetime=`{}`; reload={}; snapshot={}; restore={}; migration=`{}`; persistence=`{}`",
		json_label(&model.scope),
		json_label(&model.storage),
		json_label(&model.lifetime),
		if model.reinitialized_on_reload {
			"reinitialized"
		} else {
			"preserved"
		},
		snapshot,
		restore,
		json_label(&model.migration_policy),
		json_label(&model.persistence_policy),
	)
}

fn json_label<T: Serialize>(value: &T) -> String {
	serde_json::to_value(value)
		.ok()
		.and_then(|value| value.as_str().map(str::to_string))
		.unwrap_or_else(|| "?".into())
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

fn table_cell(s: &str) -> String {
	s.replace('\n', " ").replace('\r', " ").replace('|', "\\|")
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

		// Windows の `core.autocrlf=true` で checkout すると作業コピーが CRLF になるが、
		// `render_node_catalog_md` の出力は LF なので、バイト比較だと必ず mismatch する。
		// チェック時も BLESS 書き出し時も LF に正規化してから扱う（§χ-8.0）。
		let expected_norm = expected.replace("\r\n", "\n");

		if std::env::var_os("BLESS_NODE_CATALOG").is_some() {
			if let Some(parent) = path.parent() {
				std::fs::create_dir_all(parent).expect("create docs/manual");
			}
			std::fs::write(&path, &expected_norm).expect("write node-catalog.md");
			eprintln!("[BLESS] wrote {}", path.display());
			return;
		}

		let actual = std::fs::read_to_string(&path).unwrap_or_else(|e| {
			panic!(
				"docs/manual/node-catalog.md の読み込みに失敗: {e}\n\
				 初回生成するには `./scripts/bless-node-catalog.ps1` を実行してください。\n\
				 （fallback: `$env:BLESS_NODE_CATALOG=\"1\"; cargo test --lib node_catalog_md_up_to_date`）"
			);
		});
		let actual_norm = actual.replace("\r\n", "\n");

		if actual_norm != expected_norm {
			let diff_hint = "./scripts/bless-node-catalog.ps1 で再生成してください。";
			panic!("docs/manual/node-catalog.md が registry と不一致。{diff_hint}");
		}
	}

	#[test]
	fn node_catalog_md_includes_lf_metadata() {
		let rendered = render_node_catalog_md(&default_registry());
		assert!(rendered.contains("## Reading This Catalog"));
		assert!(rendered.contains("GUI catalog と同じ registry metadata"));
		assert!(rendered.contains("## Generated Summary"));
		assert!(rendered.contains("| Snapshot-supported nodes |"));
		assert!(rendered.contains("## Metadata Index"));
		assert!(rendered.contains("- **trace_write**"));
		assert!(rendered.contains("### Snapshot / Restore Support"));
		assert!(rendered.contains("**Metadata:** contract: inputs="));
		assert!(rendered.contains("effect: `effectful`"));
		assert!(rendered.contains("capabilities: `trace_write`"));
		assert!(rendered.contains("snapshot=explicit/json"));
	}

	#[test]
	fn markdown_table_cells_escape_pipes_and_newlines() {
		assert_eq!(table_cell("a|b\nc\rd"), "a\\|b c d");
	}

	#[test]
	fn node_catalog_summary_counts_match_registry() {
		let registry = default_registry();
		let specs = registry.all_specs();
		let categories = specs
			.iter()
			.map(|spec| spec.category.as_str())
			.collect::<std::collections::BTreeSet<_>>()
			.len();
		let effectful = specs
			.iter()
			.filter(|spec| registry.effect_class(&spec.feature) == Some("effectful"))
			.count();
		let stateful = specs
			.iter()
			.filter(|spec| registry.effect_class(&spec.feature) == Some("stateful"))
			.count();
		let capability_groups = specs
			.iter()
			.flat_map(|spec| registry.capabilities(&spec.feature))
			.collect::<std::collections::BTreeSet<_>>()
			.len();
		let snapshot_supported = specs
			.iter()
			.filter(|spec| {
				registry
					.state_model(&spec.feature)
					.map(|model| model.snapshot_supported)
					.unwrap_or(false)
			})
			.count();
		let restore_supported = specs
			.iter()
			.filter(|spec| {
				registry
					.state_model(&spec.feature)
					.map(|model| model.restore_supported)
					.unwrap_or(false)
			})
			.count();

		let rendered = render_node_catalog_md(&registry);
		for (metric, count) in [
			("Nodes", specs.len()),
			("Categories", categories),
			("Effectful nodes", effectful),
			("Stateful nodes", stateful),
			("Capability groups", capability_groups),
			("Snapshot-supported nodes", snapshot_supported),
			("Restore-supported nodes", restore_supported),
		] {
			let row = format!("| {metric} | {count} |");
			assert!(rendered.contains(&row), "missing generated summary row: {row}");
		}
	}

	#[test]
	fn node_catalog_metadata_index_counts_match_registry() {
		let registry = default_registry();
		let specs = registry.all_specs();
		let effectful = specs
			.iter()
			.filter(|spec| registry.effect_class(&spec.feature) == Some("effectful"))
			.count();
		let stateful = specs
			.iter()
			.filter(|spec| registry.effect_class(&spec.feature) == Some("stateful"))
			.count();
		let mut by_capability = std::collections::BTreeMap::<&str, usize>::new();
		for spec in &specs {
			for capability in registry.capabilities(&spec.feature) {
				*by_capability.entry(capability).or_default() += 1;
			}
		}
		let snapshot_supported = specs
			.iter()
			.filter(|spec| {
				registry
					.state_model(&spec.feature)
					.map(|model| model.snapshot_supported)
					.unwrap_or(false)
			})
			.count();
		let restore_supported = specs
			.iter()
			.filter(|spec| {
				registry
					.state_model(&spec.feature)
					.map(|model| model.restore_supported)
					.unwrap_or(false)
			})
			.count();

		let rendered = render_node_catalog_md(&registry);
		for row in [
			format!("- **effectful** ({effectful}):"),
			format!("- **stateful** ({stateful}):"),
			format!("- **snapshot** ({snapshot_supported}):"),
			format!("- **restore** ({restore_supported}):"),
		] {
			assert!(rendered.contains(&row), "missing metadata index count: {row}");
		}

		for (capability, count) in by_capability {
			let row = format!("- **{capability}** ({count}):");
			assert!(rendered.contains(&row), "missing capability index count: {row}");
		}
	}

	#[test]
	fn node_catalog_anchors_are_unique() {
		let registry = default_registry();
		let mut seen = std::collections::BTreeMap::new();
		for feature in registry.features() {
			let anchor = anchor(&feature);
			if let Some(previous) = seen.insert(anchor.clone(), feature.clone()) {
				panic!("node catalog anchor collision: {previous} and {feature} both map to {anchor}");
			}
		}
	}
}
