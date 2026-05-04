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
	out.push_str(&format!("> 型の表記: {}\n\n", type_labels(&by_category)));
	out.push_str("## Reading This Catalog\n\n");
	out.push_str("- **Index**: category ごとの通常一覧。feature 名から node 詳細へ移動するための入口です。\n");
	out.push_str("- **Metadata Index**: effect / capability / control trigger / snapshot support から node を逆引きするための一覧です。GUI catalog と同じ registry metadata から生成します。\n");
	out.push_str(
		"- 各 node section の **Metadata** line は contract summary、effect class、capability、control trigger、state model を compact に示します。\n",
	);
	out.push_str(
		"- Port / Property table の **Note** 欄に出る `enum:` / `choices:` は、GUI catalog と同じ選択肢 metadata から生成されます。\n\n",
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
	let index = collect_metadata_index(registry, specs.iter().copied());
	let total_nodes = specs.len();

	out.push_str("## Generated Summary\n\n");
	out.push_str("| Metric | Count |\n");
	out.push_str("|---|---:|\n");
	out.push_str(&format!("| Nodes | {} |\n", total_nodes));
	out.push_str(&format!("| Categories | {} |\n", by_category.len()));
	out.push_str(&format!("| Effectful nodes | {} |\n", index.effectful.len()));
	out.push_str(&format!("| Stateful nodes | {} |\n", index.stateful.len()));
	out.push_str(&format!("| Capability groups | {} |\n", index.by_capability.len()));
	out.push_str(&format!("| Control-triggerable nodes | {} |\n", index.control_triggerable.len()));
	out.push_str(&format!("| Snapshot-supported nodes | {} |\n", index.snapshot_supported.len()));
	out.push_str(&format!("| Restore-supported nodes | {} |\n\n", index.restore_supported.len()));
}

fn render_metadata_index(out: &mut String, registry: &NodeRegistry, by_category: &BTreeMap<String, Vec<NodeSpec>>) {
	let specs: Vec<&NodeSpec> = by_category.values().flat_map(|items| items.iter()).collect();
	let index = collect_metadata_index(registry, specs.iter().copied());

	out.push_str("## Metadata Index\n\n");
	out.push_str("### Effect Classes\n\n");
	out.push_str(&format!(
		"- **effectful** ({}): {}\n",
		index.effectful.len(),
		feature_links(&index.effectful)
	));
	out.push_str(&format!(
		"- **stateful** ({}): {}\n\n",
		index.stateful.len(),
		feature_links(&index.stateful)
	));
	out.push_str("### Capability Groups\n\n");
	if index.by_capability.is_empty() {
		out.push_str("- —\n\n");
	} else {
		for (capability, specs) in index.by_capability {
			out.push_str(&format!("- **{}** ({}): {}\n", capability, specs.len(), feature_links(&specs)));
		}
		out.push('\n');
	}
	out.push_str("### Control Triggering\n\n");
	out.push_str(&format!(
		"- **control triggerable** ({}): {}\n\n",
		index.control_triggerable.len(),
		feature_links(&index.control_triggerable),
	));
	out.push_str("### Snapshot / Restore Support\n\n");
	out.push_str(&format!(
		"- **snapshot** ({}): {}\n",
		index.snapshot_supported.len(),
		feature_links(&index.snapshot_supported),
	));
	out.push_str(&format!(
		"- **restore** ({}): {}\n\n",
		index.restore_supported.len(),
		feature_links(&index.restore_supported),
	));
}

struct CatalogMetadataIndex<'a> {
	effectful: Vec<&'a NodeSpec>,
	stateful: Vec<&'a NodeSpec>,
	by_capability: BTreeMap<String, Vec<&'a NodeSpec>>,
	control_triggerable: Vec<&'a NodeSpec>,
	snapshot_supported: Vec<&'a NodeSpec>,
	restore_supported: Vec<&'a NodeSpec>,
}

fn collect_metadata_index<'a>(registry: &NodeRegistry, specs: impl IntoIterator<Item = &'a NodeSpec>) -> CatalogMetadataIndex<'a> {
	let mut index = CatalogMetadataIndex {
		effectful: Vec::new(),
		stateful: Vec::new(),
		by_capability: BTreeMap::new(),
		control_triggerable: Vec::new(),
		snapshot_supported: Vec::new(),
		restore_supported: Vec::new(),
	};

	for spec in specs {
		match registry.effect_class(&spec.feature).unwrap_or("unknown") {
			"effectful" => index.effectful.push(spec),
			"stateful" => index.stateful.push(spec),
			_ => {}
		}
		for cap in registry.capabilities(&spec.feature) {
			index.by_capability.entry(cap.to_string()).or_default().push(spec);
		}
		if registry.is_control_triggerable(&spec.feature) {
			index.control_triggerable.push(spec);
		}
		if let Some(model) = registry.state_model(&spec.feature) {
			if model.snapshot_supported {
				index.snapshot_supported.push(spec);
			}
			if model.restore_supported {
				index.restore_supported.push(spec);
			}
		}
	}

	index
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

fn type_labels(by_category: &BTreeMap<String, Vec<NodeSpec>>) -> String {
	let mut labels = std::collections::BTreeSet::new();
	for spec in by_category.values().flat_map(|items| items.iter()) {
		for port in spec.inputs.iter().chain(spec.outputs.iter()) {
			labels.insert(port.ty.to_string());
		}
		for property in &spec.properties {
			labels.insert(property.ty.to_string());
		}
	}
	labels.into_iter().map(|label| format!("`{label}`")).collect::<Vec<_>>().join(" / ")
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
				table_cell(&property_note(p)),
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
		"\n**Metadata:** contract: {}; effect: `{}`; capabilities: {}; trigger: {}; state: {}\n\n",
		contract_summary(spec),
		effect_class,
		capability_summary(&capabilities),
		trigger_summary(registry, spec),
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

fn trigger_summary(registry: &NodeRegistry, spec: &NodeSpec) -> &'static str {
	if registry.is_control_triggerable(&spec.feature) {
		"control"
	} else {
		"—"
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
	if let Some(variants) = p.closed_string_variants.as_ref().filter(|variants| !variants.is_empty()) {
		parts.push(format!("enum: {}", choice_labels(variants)));
	}
	if let Some(d) = &p.description {
		parts.push(d.clone());
	}
	parts.join("; ").replace('\n', " ")
}

fn property_default(p: &PropertySpec) -> String {
	compact_json(&p.default.0)
}

fn property_note(p: &PropertySpec) -> String {
	let mut parts: Vec<String> = Vec::new();
	if let Some(choices) = p.choices.as_ref().filter(|choices| !choices.is_empty()) {
		parts.push(format!("choices: {}", choice_labels(choices)));
	}
	if let Some(validator) = &p.validator {
		parts.push(format!("validator: `{}`", validator));
	}
	if let Some(description) = &p.description {
		parts.push(description.clone());
	}
	parts.join("; ").replace('\n', " ")
}

fn choice_labels(values: &[String]) -> String {
	values.iter().map(|value| format!("`{}`", value)).collect::<Vec<_>>().join(", ")
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
		assert!(rendered.contains("`datetime`"));
		assert!(rendered.contains("`quantity`"));
		assert!(rendered.contains("`table`"));
		assert!(rendered.contains("GUI catalog と同じ registry metadata"));
		assert!(rendered.contains("`enum:` / `choices:`"));
		assert!(rendered.contains("## Generated Summary"));
		assert!(rendered.contains("| Control-triggerable nodes |"));
		assert!(rendered.contains("| Snapshot-supported nodes |"));
		assert!(rendered.contains("## Metadata Index"));
		assert!(rendered.contains("- **trace_write**"));
		assert!(rendered.contains("### Control Triggering"));
		assert!(rendered.contains("- **control triggerable**"));
		assert!(rendered.contains("### Snapshot / Restore Support"));
		assert!(rendered.contains("**Metadata:** contract: inputs="));
		assert!(rendered.contains("effect: `effectful`"));
		assert!(rendered.contains("capabilities: `trace_write`"));
		assert!(rendered.contains("trigger: control"));
		assert!(rendered.contains("snapshot=explicit/json"));
		assert!(rendered.contains("enum: `aivis_speech`, `bouyomichan`, `coeiroink`, `os`, `voicepeak`, `voicevox`"));
		assert!(rendered.contains("choices: `rfc3339`, `iso8601_compact`, `unix_seconds`, `unix_millis`, `custom`"));
	}

	#[test]
	fn markdown_table_cells_escape_pipes_and_newlines() {
		assert_eq!(table_cell("a|b\nc\rd"), "a\\|b c d");
	}

	#[test]
	fn rendered_node_catalog_is_lf_only_with_final_newline() {
		let rendered = render_node_catalog_md(&default_registry());
		assert!(!rendered.contains('\r'), "generated node catalog should be LF-only");
		assert!(rendered.ends_with('\n'), "generated node catalog should end with a newline");
	}

	fn markdown_table_delimiter_count(line: &str) -> usize {
		let mut escaped = false;
		let mut count = 0;
		for ch in line.chars() {
			if escaped {
				escaped = false;
				continue;
			}
			match ch {
				'\\' => escaped = true,
				'|' => count += 1,
				_ => {}
			}
		}
		count
	}

	#[test]
	fn generated_markdown_tables_keep_consistent_shape() {
		let rendered = render_node_catalog_md(&default_registry());
		let mut expected_delimiters = None;
		for line in rendered.lines() {
			if !line.starts_with('|') {
				expected_delimiters = None;
				continue;
			}

			let delimiters = markdown_table_delimiter_count(line);
			if let Some(expected) = expected_delimiters {
				assert_eq!(delimiters, expected, "inconsistent markdown table row: {line}");
			} else {
				expected_delimiters = Some(delimiters);
			}
		}
	}

	#[test]
	fn node_catalog_summary_counts_match_registry() {
		let registry = default_registry();
		let specs = registry.all_specs();
		let index = collect_metadata_index(&registry, specs.iter());
		let categories = specs
			.iter()
			.map(|spec| spec.category.as_str())
			.collect::<std::collections::BTreeSet<_>>()
			.len();

		let rendered = render_node_catalog_md(&registry);
		for (metric, count) in [
			("Nodes", specs.len()),
			("Categories", categories),
			("Effectful nodes", index.effectful.len()),
			("Stateful nodes", index.stateful.len()),
			("Capability groups", index.by_capability.len()),
			("Control-triggerable nodes", index.control_triggerable.len()),
			("Snapshot-supported nodes", index.snapshot_supported.len()),
			("Restore-supported nodes", index.restore_supported.len()),
		] {
			let row = format!("| {metric} | {count} |");
			assert!(rendered.contains(&row), "missing generated summary row: {row}");
		}
	}

	#[test]
	fn node_catalog_metadata_index_counts_match_registry() {
		let registry = default_registry();
		let specs = registry.all_specs();
		let index = collect_metadata_index(&registry, specs.iter());

		let rendered = render_node_catalog_md(&registry);
		for row in [
			format!("- **effectful** ({}):", index.effectful.len()),
			format!("- **stateful** ({}):", index.stateful.len()),
			format!("- **control triggerable** ({}):", index.control_triggerable.len()),
			format!("- **snapshot** ({}):", index.snapshot_supported.len()),
			format!("- **restore** ({}):", index.restore_supported.len()),
		] {
			assert!(rendered.contains(&row), "missing metadata index count: {row}");
		}

		for (capability, specs) in index.by_capability {
			let row = format!("- **{capability}** ({}):", specs.len());
			assert!(rendered.contains(&row), "missing capability index count: {row}");
		}
	}

	#[test]
	fn node_catalog_index_and_sections_cover_all_registry_features() {
		let registry = default_registry();
		let rendered = render_node_catalog_md(&registry);
		for spec in registry.all_specs() {
			let index_entry = format!("  - [`{}`](#{}) — {}", spec.feature, anchor(&spec.feature), spec.title);
			assert!(rendered.contains(&index_entry), "missing catalog index entry: {index_entry}");

			let section_heading = format!("### `{}`", spec.feature);
			assert!(rendered.contains(&section_heading), "missing catalog section: {section_heading}");
		}
	}

	#[test]
	fn node_catalog_enum_metadata_covers_all_registry_choices() {
		let registry = default_registry();
		let rendered = render_node_catalog_md(&registry);
		for spec in registry.all_specs() {
			for port in spec.inputs.iter().chain(spec.outputs.iter()) {
				if let Some(variants) = port.closed_string_variants.as_ref().filter(|variants| !variants.is_empty()) {
					let note = format!("enum: {}", choice_labels(variants));
					assert!(
						rendered.contains(&note),
						"missing port enum note for {}:{}: {note}",
						spec.feature,
						port.name
					);
				}
			}

			for property in &spec.properties {
				if let Some(choices) = property.choices.as_ref().filter(|choices| !choices.is_empty()) {
					let note = format!("choices: {}", choice_labels(choices));
					assert!(
						rendered.contains(&note),
						"missing property choices note for {}:{}: {note}",
						spec.feature,
						property.name
					);
				}
			}
		}
	}

	#[test]
	fn node_catalog_type_labels_cover_all_registry_socket_types() {
		let registry = default_registry();
		let specs = registry.all_specs();
		let rendered = render_node_catalog_md(&registry);
		let type_line = rendered
			.lines()
			.find(|line| line.starts_with("> 型の表記:"))
			.expect("generated catalog should include type label line");

		for spec in specs {
			for port in spec.inputs.iter().chain(spec.outputs.iter()) {
				let label = format!("`{}`", port.ty);
				assert!(
					type_line.contains(&label),
					"missing port type label for {}:{}: {label}",
					spec.feature,
					port.name
				);
			}
			for property in &spec.properties {
				let label = format!("`{}`", property.ty);
				assert!(
					type_line.contains(&label),
					"missing property type label for {}:{}: {label}",
					spec.feature,
					property.name
				);
			}
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
