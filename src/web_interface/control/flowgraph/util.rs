//! Flowgraph Control API 共通: JSON エラー、パス安全、node-catalog 用 UI ヒント、ディスク補助。

use std::path::{Path, PathBuf};

use actix_web::HttpResponse;

use crate::flowgraph::node::json_to_socket_value;
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::flowgraph::FlowgraphRuntime;
use crate::SharedState;

pub(crate) fn err_json(status: actix_web::http::StatusCode, code: &str, detail: impl std::fmt::Display) -> HttpResponse {
	HttpResponse::build(status).json(serde_json::json!({
		"error": code,
		"detail": detail.to_string(),
	}))
}

fn shorten_unit_label_chars(s: &str, max_chars: usize) -> String {
	let count = s.chars().count();
	if count <= max_chars {
		return s.to_string();
	}
	let take = max_chars.saturating_sub(1);
	s.chars().take(take).collect::<String>() + "…"
}

/// Phase ξ-5: GUI が Quantity ポートの badge / tooltip に使うヒントを JSON に注入する。
/// `default` が dimensionless のときはキーを付けない（既存クライアント互換）。
pub(crate) fn enrich_quantity_port_ui_hints(port: &mut serde_json::Value) {
	let Some(obj) = port.as_object_mut() else {
		return;
	};
	let Some(ty) = obj.get("ty").and_then(|t| t.as_str()) else {
		return;
	};
	if ty != "quantity" {
		return;
	}
	let Some(SocketValue::Quantity(q)) = obj.get("default").and_then(|d| json_to_socket_value(&SocketType::Quantity, d)) else {
		return;
	};
	let dim = q.dimension();
	if dim.is_dimensionless() {
		return;
	}
	let dim_s = dim.canonical();
	let full = q.unit.canonical();
	let badge = shorten_unit_label_chars(&full, 14);
	obj.insert("quantity_dim".to_string(), serde_json::Value::String(dim_s));
	obj.insert("quantity_unit_badge".to_string(), serde_json::Value::String(badge));
	obj.insert("quantity_unit_full".to_string(), serde_json::Value::String(full));
}

fn port_contracts(v: &serde_json::Value, key: &str) -> Vec<serde_json::Value> {
	v.get(key)
		.and_then(|x| x.as_array())
		.map(|arr| {
			arr.iter()
				.filter_map(|p| {
					let obj = p.as_object()?;
					let mut out = serde_json::Map::new();
					for (from, to) in [
						("name", "name"),
						("label", "label"),
						("ty", "type"),
						("direction", "direction"),
						("is_exec", "exec"),
						("optional", "optional"),
						("multi", "multi"),
						("default", "default"),
						("closed_string_variants", "enum"),
					] {
						if let Some(value) = obj.get(from) {
							out.insert(to.to_string(), value.clone());
						}
					}
					Some(serde_json::Value::Object(out))
				})
				.collect()
		})
		.unwrap_or_default()
}

fn property_contracts(v: &serde_json::Value) -> Vec<serde_json::Value> {
	v.get("properties")
		.and_then(|x| x.as_array())
		.map(|arr| {
			arr.iter()
				.filter_map(|p| {
					let obj = p.as_object()?;
					let mut out = serde_json::Map::new();
					for (from, to) in [
						("name", "name"),
						("label", "label"),
						("ty", "type"),
						("default", "default"),
						("required", "required"),
						("validator", "validator"),
						("choices", "enum"),
					] {
						if let Some(value) = obj.get(from) {
							out.insert(to.to_string(), value.clone());
						}
					}
					Some(serde_json::Value::Object(out))
				})
				.collect()
		})
		.unwrap_or_default()
}

/// LF-1: 既存 NodeSpec から node signature / contract を派生する。
///
/// 現段階では `inputs` / `outputs` / `properties` を壊さず、GUI と将来の library
/// signature が同じ machine-readable 語彙を参照できるよう catalog JSON にだけ注入する。
pub(crate) fn enrich_contract_json(v: &mut serde_json::Value) {
	let feature = v.get("feature").cloned().unwrap_or(serde_json::Value::Null);
	let inputs = port_contracts(v, "inputs");
	let outputs = port_contracts(v, "outputs");
	let properties = property_contracts(v);
	let has_exec_input = inputs.iter().any(|p| p.get("exec").and_then(|x| x.as_bool()) == Some(true));
	let has_exec_output = outputs.iter().any(|p| p.get("exec").and_then(|x| x.as_bool()) == Some(true));
	let Some(obj) = v.as_object_mut() else {
		return;
	};
	obj.insert(
		"contract".to_string(),
		serde_json::json!({
			"version": 1,
			"kind": "node",
			"feature": feature,
			"inputs": inputs,
			"outputs": outputs,
			"properties": properties,
			"summary": {
				"input_count": inputs.len(),
				"output_count": outputs.len(),
				"property_count": properties.len(),
				"has_exec_input": has_exec_input,
				"has_exec_output": has_exec_output,
			}
		}),
	);
}

/// LF-2: catalog JSON に副作用クラスと capability summary を注入する。
///
/// まだ policy enforcement は行わない。GUI 表示、graph summary、mock capability 設計のための
/// read-only metadata として扱う。
pub(crate) fn enrich_effect_metadata_json(reg: &crate::flowgraph::registry::NodeRegistry, v: &mut serde_json::Value) {
	let Some(obj) = v.as_object_mut() else {
		return;
	};
	let feature = obj.get("feature").and_then(|f| f.as_str()).unwrap_or("");
	let category = obj.get("category").and_then(|c| c.as_str()).unwrap_or("");
	let effect_class = reg.effect_class(feature).unwrap_or("unknown");
	let caps = crate::flowgraph::registry::infer_capabilities(feature, category, effect_class);
	obj.insert("effect_class".to_string(), serde_json::Value::String(effect_class.to_string()));
	obj.insert(
		"capabilities".to_string(),
		serde_json::Value::Array(caps.into_iter().map(|s| serde_json::Value::String(s.to_string())).collect()),
	);
}

/// LF-7: catalog JSON に state model metadata を注入する。
///
/// 現段階では read-only な宣言に留め、stateful node の state slot が node instance に属し、
/// program reload で再初期化される volatile state であることを GUI / 将来の snapshot 層へ明示する。
pub(crate) fn enrich_state_model_json(reg: &crate::flowgraph::registry::NodeRegistry, v: &mut serde_json::Value) {
	let Some(obj) = v.as_object_mut() else {
		return;
	};
	let feature = obj.get("feature").and_then(|f| f.as_str()).unwrap_or("");
	let state_model = reg
		.state_model(feature)
		.unwrap_or_else(|| crate::flowgraph::FlowgraphStateModel::for_effect_class("unknown"));
	obj.insert(
		"state_model".to_string(),
		serde_json::to_value(state_model).unwrap_or(serde_json::Value::Null),
	);
}

/// node-catalog の各 spec JSON に control_triggerable + Quantity UI ヒント + contract/effect metadata を注入する。
pub(crate) fn enrich_node_catalog_spec_json(reg: &crate::flowgraph::registry::NodeRegistry, v: &mut serde_json::Value) {
	let Some(obj) = v.as_object_mut() else {
		return;
	};
	let feature = obj.get("feature").and_then(|f| f.as_str()).unwrap_or("");
	obj.insert(
		"control_triggerable".to_string(),
		serde_json::Value::Bool(reg.is_control_triggerable(feature)),
	);
	for key in ["inputs", "outputs"] {
		if let Some(serde_json::Value::Array(arr)) = obj.get_mut(key) {
			for item in arr.iter_mut() {
				enrich_quantity_port_ui_hints(item);
			}
		}
	}
	enrich_contract_json(v);
	enrich_effect_metadata_json(reg, v);
	enrich_state_model_json(reg, v);
}

/// `state.flowgraph` と `conf.flowgraph_dir` を取り出す。dir 未設定なら 500。
pub(crate) async fn flowgraph_dir(state: &SharedState) -> Result<(PathBuf, Option<FlowgraphRuntime>), HttpResponse> {
	let fg = state.read().await.flowgraph.clone();
	let rt_guard = fg.read().await;
	let Some(rt) = rt_guard.as_ref() else {
		return Err(err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定のため Flowgraph API は無効です。",
		));
	};
	Ok((rt.root_dir.clone(), Some(rt.clone())))
}

/// fq 文字列を検証し、`<root>/<fq>.flowgraph.toml` の絶対パスを返す。
///
/// - 空 / `..` / 絶対パスは拒否。
/// - 最終的に root 配下であることを確認。root 自体は存在しなくても fq 解決は通す
///   （ファイル新規作成時に root も作るため）。
pub(crate) fn fq_to_file_path(root: &Path, fq: &str) -> Result<PathBuf, HttpResponse> {
	let fq = fq.trim_matches('/').trim();
	if fq.is_empty() {
		return Err(err_json(actix_web::http::StatusCode::BAD_REQUEST, "empty_fq", "fq が空です"));
	}
	for seg in fq.split('/') {
		if seg.is_empty() || seg == "." || seg == ".." {
			return Err(err_json(
				actix_web::http::StatusCode::BAD_REQUEST,
				"invalid_fq",
				format!("fq に不正なセグメントが含まれます: '{seg}' in '{fq}'"),
			));
		}
	}
	if fq.contains('\\') {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_fq",
			"fq にバックスラッシュは使えません",
		));
	}
	// Windows のドライブ文字プレフィックスなども Path::new で絶対判定される。
	let rel = Path::new(fq);
	if rel.is_absolute() {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_fq",
			"fq は相対パスでなければなりません",
		));
	}
	let filename = format!("{}.flowgraph.toml", rel.file_name().and_then(|s| s.to_str()).unwrap_or(""));
	let file_path = match rel.parent() {
		Some(p) if !p.as_os_str().is_empty() => root.join(p).join(filename),
		_ => root.join(filename),
	};
	Ok(file_path)
}

/// 「確定したパスが root 配下であること」を canonicalize 後に検査。
///
/// 存在しないファイル（新規作成予定）でも動くよう、親ディレクトリを基準に判定する。
pub(crate) fn ensure_within_root(root: &Path, file_path: &Path) -> Result<(), HttpResponse> {
	// 親の canonicalize 値が root の canonicalize 値の接頭辞であること。
	let root_canon = match std::fs::canonicalize(root) {
		Ok(p) => p,
		Err(_) => {
			// root 自体が未作成（opt-in 未使用 dir）のケース。ここでは `ancestors()` の lexical 比較で妥協。
			return lexical_within(root, file_path);
		}
	};
	let parent = file_path.parent().unwrap_or(file_path);
	let parent_canon = match std::fs::canonicalize(parent) {
		Ok(p) => p,
		Err(_) => {
			// 親がまだ無い（深いサブディレクトリ新規）ケース。root と合流するまで遡る。
			let mut cur: &Path = parent;
			loop {
				match std::fs::canonicalize(cur) {
					Ok(abs) => {
						if !abs.starts_with(&root_canon) {
							return Err(err_json(
								actix_web::http::StatusCode::BAD_REQUEST,
								"path_traversal",
								format!("fq が flowgraph_dir の外を指します: {}", file_path.display()),
							));
						}
						return Ok(());
					}
					Err(_) => {
						cur = match cur.parent() {
							Some(p) => p,
							None => {
								return Err(err_json(
									actix_web::http::StatusCode::BAD_REQUEST,
									"path_unresolvable",
									format!("パスを正規化できません: {}", file_path.display()),
								))
							}
						};
					}
				}
			}
		}
	};
	if !parent_canon.starts_with(&root_canon) {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"path_traversal",
			format!("fq が flowgraph_dir の外を指します: {}", file_path.display()),
		));
	}
	Ok(())
}

/// `canonicalize` 使えないケース向けの lexical 比較。`file_path` が `root` 直下の構造かをざっくり見る。
fn lexical_within(root: &Path, file_path: &Path) -> Result<(), HttpResponse> {
	if file_path.starts_with(root) {
		Ok(())
	} else {
		Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"path_traversal",
			format!("fq が flowgraph_dir の外を指します: {}", file_path.display()),
		))
	}
}

pub(crate) fn root_relative(root: &Path, file: &Path) -> String {
	file.strip_prefix(root)
		.map(|p| p.to_string_lossy().replace('\\', "/"))
		.unwrap_or_else(|_| file.display().to_string())
}

pub(crate) fn make_backup(file: &Path) -> std::io::Result<String> {
	let ts = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
	let name = file.file_name().and_then(|s| s.to_str()).unwrap_or("unknown.flowgraph.toml");
	let bak_name = format!("{name}.bak-{ts}");
	let bak_path = file.with_file_name(&bak_name);
	std::fs::copy(file, &bak_path)?;
	Ok(bak_name)
}

/// プラットフォーム既定のエディタ/ビューアで file を開く（fire-and-forget）。
pub(crate) fn spawn_os_opener(file: &Path) -> (String, bool) {
	let path = file.display().to_string();
	#[cfg(target_os = "windows")]
	{
		let cmd = format!("cmd /C start \"\" {path:?}");
		let spawned = std::process::Command::new("cmd").args(["/C", "start", "", &path]).spawn().is_ok();
		(cmd, spawned)
	}
	#[cfg(target_os = "macos")]
	{
		let cmd = format!("open {path:?}");
		let spawned = std::process::Command::new("open").arg(&path).spawn().is_ok();
		(cmd, spawned)
	}
	#[cfg(all(unix, not(target_os = "macos")))]
	{
		let cmd = format!("xdg-open {path:?}");
		let spawned = std::process::Command::new("xdg-open").arg(&path).spawn().is_ok();
		(cmd, spawned)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fq_to_file_path_basic() {
		let root = Path::new("/abs/flowgraph");
		assert_eq!(
			fq_to_file_path(root, "main").unwrap(),
			Path::new("/abs/flowgraph/main.flowgraph.toml")
		);
		assert_eq!(
			fq_to_file_path(root, "chat-echo/main").unwrap(),
			Path::new("/abs/flowgraph/chat-echo/main.flowgraph.toml")
		);
		// 末尾 / も trim
		assert_eq!(
			fq_to_file_path(root, "/main/").unwrap(),
			Path::new("/abs/flowgraph/main.flowgraph.toml")
		);
	}

	#[test]
	fn fq_to_file_path_rejects_traversal() {
		let root = Path::new("/abs/flowgraph");
		assert!(fq_to_file_path(root, "").is_err());
		assert!(fq_to_file_path(root, "../sneak").is_err());
		assert!(fq_to_file_path(root, "dir/../sneak").is_err());
		assert!(fq_to_file_path(root, "././a").is_err());
		assert!(fq_to_file_path(root, "dir\\sneak").is_err());
	}

	#[test]
	fn ensure_within_root_lexical_when_root_missing() {
		// root が存在しないときは lexical 比較にフォールバック。
		let root = Path::new("C:/nonexistent-vac-test-xyz");
		let inside = root.join("main.flowgraph.toml");
		assert!(ensure_within_root(root, &inside).is_ok());

		let outside = Path::new("C:/other/main.flowgraph.toml");
		assert!(ensure_within_root(root, outside).is_err());
	}

	#[test]
	fn enrich_quantity_port_ui_hint_from_default_object() {
		let mut port = serde_json::json!({
			"name": "result",
			"label": "Result",
			"ty": "quantity",
			"direction": "output",
			"is_exec": false,
			"optional": false,
			"default": { "value": 9.81, "unit": "m/s^2" },
			"multi": false
		});
		enrich_quantity_port_ui_hints(&mut port);
		assert_eq!(port["quantity_dim"].as_str().unwrap(), "L·T^-2");
		assert!(port["quantity_unit_badge"].as_str().unwrap().len() > 0);
		assert!(port["quantity_unit_full"].as_str().is_some());
	}

	#[test]
	fn registry_marks_glossary_nodes_triggerable() {
		use crate::flowgraph::registry::registry;

		let r = registry();
		assert!(r.is_control_triggerable("flowgraph.glossary.learn"));
		assert!(r.is_control_triggerable("flowgraph.glossary.forget"));
		assert!(!r.is_control_triggerable("flowgraph.dictionary.learn"));
		assert!(!r.is_control_triggerable("flowgraph.dictionary.forget"));
		// 既定はオプトインされていないはず。
		assert!(!r.is_control_triggerable("flowgraph.literal.string"));
		assert!(!r.is_control_triggerable("flowgraph.util.log"));
		assert!(!r.is_control_triggerable("flowgraph.channel.emit"));
		// 未登録は無条件 false。
		assert!(!r.is_control_triggerable("flowgraph.does.not.exist"));
	}

	#[test]
	fn node_catalog_json_injects_control_triggerable_flag() {
		use crate::flowgraph::registry::registry;

		let reg = registry();
		let specs = reg.all_specs();
		let values: Vec<serde_json::Value> = specs
			.iter()
			.map(|s| {
				let mut v = serde_json::to_value(s).unwrap();
				enrich_node_catalog_spec_json(reg, &mut v);
				v
			})
			.collect();

		let by_feature: std::collections::HashMap<&str, &serde_json::Value> =
			values.iter().map(|v| (v["feature"].as_str().unwrap(), v)).collect();

		// opt-in 済み: glossary.learn / .forget
		for f in ["flowgraph.glossary.learn", "flowgraph.glossary.forget"] {
			let spec = by_feature.get(f).unwrap_or_else(|| panic!("{f} が node-catalog にいない"));
			assert_eq!(
				spec["control_triggerable"].as_bool(),
				Some(true),
				"{f} は control_triggerable=true であるべき"
			);
		}

		// 典型的な non-opt-in: literal / log / tts / glossary.replace
		for f in [
			"flowgraph.literal.string",
			"flowgraph.util.log",
			"flowgraph.tts.speak",
			"flowgraph.glossary.replace",
		] {
			let spec = by_feature.get(f).unwrap_or_else(|| panic!("{f} が node-catalog にいない"));
			assert_eq!(
				spec["control_triggerable"].as_bool(),
				Some(false),
				"{f} は control_triggerable=false であるべき"
			);
		}
	}

	#[test]
	fn node_catalog_json_injects_lf1_contract() {
		use crate::flowgraph::registry::registry;

		let reg = registry();
		let spec = reg.spec("flowgraph.glossary.learn").expect("glossary.learn が registry に必要");
		let mut v = serde_json::to_value(&spec).unwrap();
		enrich_node_catalog_spec_json(reg, &mut v);

		let contract = &v["contract"];
		assert_eq!(contract["version"].as_i64(), Some(1));
		assert_eq!(contract["kind"].as_str(), Some("node"));
		assert_eq!(contract["feature"].as_str(), Some("flowgraph.glossary.learn"));
		assert_eq!(contract["summary"]["has_exec_input"].as_bool(), Some(true));
		assert_eq!(contract["summary"]["has_exec_output"].as_bool(), Some(true));

		let inputs = contract["inputs"].as_array().expect("inputs contract");
		let dictionary = inputs
			.iter()
			.find(|p| p["name"].as_str() == Some("dictionary"))
			.expect("dictionary input");
		assert_eq!(dictionary["type"].as_str(), Some("table"));
		assert_eq!(dictionary["optional"].as_bool(), Some(true));

		let outputs = contract["outputs"].as_array().expect("outputs contract");
		assert!(outputs
			.iter()
			.any(|p| p["name"].as_str() == Some("updated_dictionary") && p["type"].as_str() == Some("table")));
	}

	#[test]
	fn node_catalog_json_injects_lf2_effect_metadata() {
		use crate::flowgraph::registry::registry;

		let reg = registry();
		let mut specs = std::collections::HashMap::new();
		for feature in [
			"flowgraph.literal.string",
			"flowgraph.glossary.match",
			"flowgraph.table.write_tsv",
			"flowgraph.twitch.chat_send",
			"flowgraph.obs.set_current_program_scene",
		] {
			let spec = reg.spec(feature).unwrap_or_else(|| panic!("{feature} が registry に必要"));
			let mut v = serde_json::to_value(&spec).unwrap();
			enrich_node_catalog_spec_json(reg, &mut v);
			specs.insert(feature, v);
		}

		assert_eq!(specs["flowgraph.literal.string"]["effect_class"].as_str(), Some("pure"));
		assert_eq!(specs["flowgraph.glossary.match"]["effect_class"].as_str(), Some("stateful"));
		assert_eq!(specs["flowgraph.table.write_tsv"]["effect_class"].as_str(), Some("effectful"));
		assert!(specs["flowgraph.table.write_tsv"]["capabilities"]
			.as_array()
			.unwrap()
			.contains(&serde_json::json!("file_write")));
		assert!(specs["flowgraph.twitch.chat_send"]["capabilities"]
			.as_array()
			.unwrap()
			.contains(&serde_json::json!("twitch_api")));
		assert!(specs["flowgraph.obs.set_current_program_scene"]["capabilities"]
			.as_array()
			.unwrap()
			.contains(&serde_json::json!("obs_control")));
	}

	#[test]
	fn node_catalog_json_injects_lf7_state_model() {
		use crate::flowgraph::registry::registry;

		let reg = registry();
		let mut specs = std::collections::HashMap::new();
		for feature in [
			"flowgraph.literal.string",
			"flowgraph.state.int_counter",
			"flowgraph.util.rate_limit",
			"flowgraph.table.write_tsv",
		] {
			let spec = reg.spec(feature).unwrap_or_else(|| panic!("{feature} が registry に必要"));
			let mut v = serde_json::to_value(&spec).unwrap();
			enrich_node_catalog_spec_json(reg, &mut v);
			specs.insert(feature, v);
		}

		let counter = &specs["flowgraph.state.int_counter"]["state_model"];
		assert_eq!(counter["version"].as_i64(), Some(1));
		assert_eq!(counter["stateful"].as_bool(), Some(true));
		assert_eq!(counter["scope"].as_str(), Some("node_instance"));
		assert_eq!(counter["storage"].as_str(), Some("volatile"));
		assert_eq!(counter["lifetime"].as_str(), Some("program_instance"));
		assert_eq!(counter["reinitialized_on_reload"].as_bool(), Some(true));
		assert_eq!(counter["snapshot_supported"].as_bool(), Some(true));
		assert_eq!(counter["snapshot_policy"].as_str(), Some("explicit"));
		assert_eq!(counter["snapshot_format"].as_str(), Some("json"));
		assert_eq!(counter["restore_supported"].as_bool(), Some(true));
		assert_eq!(counter["restore_policy"].as_str(), Some("explicit"));
		assert_eq!(counter["migration_policy"].as_str(), Some("none"));
		assert_eq!(counter["persistence_policy"].as_str(), Some("none"));

		let rate_limit = &specs["flowgraph.util.rate_limit"]["state_model"];
		assert_eq!(rate_limit["stateful"].as_bool(), Some(true));
		assert_eq!(rate_limit["snapshot_supported"].as_bool(), Some(true));
		assert_eq!(rate_limit["snapshot_policy"].as_str(), Some("explicit"));
		assert_eq!(rate_limit["snapshot_format"].as_str(), Some("json"));
		assert_eq!(rate_limit["restore_supported"].as_bool(), Some(true));
		assert_eq!(rate_limit["restore_policy"].as_str(), Some("explicit"));

		for feature in ["flowgraph.literal.string", "flowgraph.table.write_tsv"] {
			let state_model = &specs[feature]["state_model"];
			assert_eq!(state_model["stateful"].as_bool(), Some(false));
			assert_eq!(state_model["scope"].as_str(), Some("none"));
			assert_eq!(state_model["storage"].as_str(), Some("none"));
			assert_eq!(state_model["reinitialized_on_reload"].as_bool(), Some(false));
			assert_eq!(state_model["snapshot_policy"].as_str(), Some("unsupported"));
			assert_eq!(state_model["snapshot_format"].as_str(), Some("none"));
			assert_eq!(state_model["restore_supported"].as_bool(), Some(false));
			assert_eq!(state_model["restore_policy"].as_str(), Some("unsupported"));
			assert_eq!(state_model["migration_policy"].as_str(), Some("none"));
			assert_eq!(state_model["persistence_policy"].as_str(), Some("none"));
		}
	}

	#[test]
	fn generated_manual_catalog_summary_matches_control_api_catalog_metadata() {
		use crate::flowgraph::docs::render_node_catalog_md;
		use crate::flowgraph::registry::registry;

		let reg = registry();
		let values: Vec<serde_json::Value> = reg
			.all_specs()
			.iter()
			.map(|spec| {
				let mut value = serde_json::to_value(spec).unwrap();
				enrich_node_catalog_spec_json(reg, &mut value);
				value
			})
			.collect();

		let effectful = values
			.iter()
			.filter(|value| value["effect_class"].as_str() == Some("effectful"))
			.count();
		let stateful = values
			.iter()
			.filter(|value| value["effect_class"].as_str() == Some("stateful"))
			.count();
		let control_triggerable = values
			.iter()
			.filter(|value| value["control_triggerable"].as_bool() == Some(true))
			.count();
		let snapshot_supported = values
			.iter()
			.filter(|value| value["state_model"]["snapshot_supported"].as_bool() == Some(true))
			.count();
		let restore_supported = values
			.iter()
			.filter(|value| value["state_model"]["restore_supported"].as_bool() == Some(true))
			.count();
		let mut by_capability = std::collections::BTreeMap::<String, usize>::new();
		for value in &values {
			for capability in value["capabilities"].as_array().into_iter().flatten() {
				let capability = capability.as_str().expect("capability should be a string");
				*by_capability.entry(capability.to_string()).or_default() += 1;
			}
		}

		let rendered = render_node_catalog_md(reg);
		for row in [
			format!("| Effectful nodes | {effectful} |"),
			format!("| Stateful nodes | {stateful} |"),
			format!("| Capability groups | {} |", by_capability.len()),
			format!("| Control-triggerable nodes | {control_triggerable} |"),
			format!("| Snapshot-supported nodes | {snapshot_supported} |"),
			format!("| Restore-supported nodes | {restore_supported} |"),
		] {
			assert!(
				rendered.contains(&row),
				"manual catalog summary should match Control API metadata: {row}"
			);
		}

		for (capability, count) in by_capability {
			let row = format!("- **{capability}** ({count}):");
			assert!(
				rendered.contains(&row),
				"manual capability index should match Control API metadata: {row}"
			);
		}
	}
}
