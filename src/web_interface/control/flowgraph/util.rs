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

/// node-catalog の各 spec JSON に control_triggerable + Quantity UI ヒントを注入する。
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
	fn registry_marks_dictionary_nodes_triggerable() {
		use crate::flowgraph::registry::registry;

		let r = registry();
		assert!(r.is_control_triggerable("flowgraph.dictionary.learn"));
		assert!(r.is_control_triggerable("flowgraph.dictionary.forget"));
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

		// opt-in 済み: dictionary.learn / .forget
		for f in ["flowgraph.dictionary.learn", "flowgraph.dictionary.forget"] {
			let spec = by_feature.get(f).unwrap_or_else(|| panic!("{f} が node-catalog にいない"));
			assert_eq!(
				spec["control_triggerable"].as_bool(),
				Some(true),
				"{f} は control_triggerable=true であるべき"
			);
		}

		// 典型的な non-opt-in: literal / log / tts / dictionary.replace
		for f in [
			"flowgraph.literal.string",
			"flowgraph.util.log",
			"flowgraph.tts.speak",
			"flowgraph.dictionary.replace",
		] {
			let spec = by_feature.get(f).unwrap_or_else(|| panic!("{f} が node-catalog にいない"));
			assert_eq!(
				spec["control_triggerable"].as_bool(),
				Some(false),
				"{f} は control_triggerable=false であるべき"
			);
		}
	}
}
