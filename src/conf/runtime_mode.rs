//! RM-1: `conf.toml` の `[modes.*]`（Runtime Mode 宣言）の serde とロード時検証。
//!
//! 正本の意味論は `docs/roadmap/runtime-mode-roadmap.md`。Mode Manager 本体は後続 PR。

use super::{bail, Conf, Result, RunWith};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 1 モード分の Flowgraph グループの desired state（有効化 / 無効化リスト）。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct FlowgraphGroupsModeSpec {
	#[serde(default)]
	pub enable: Vec<String>,
	#[serde(default)]
	pub disable: Vec<String>,
}

/// Managed App（`run_with` 由来）に対する desired state の宣言片。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ManagedAppsModeDirective {
	#[serde(default)]
	pub start: Vec<String>,
	#[serde(default)]
	pub stop: Vec<String>,
	#[serde(default)]
	pub minimize: Vec<String>,
	#[serde(default)]
	pub leave: Vec<String>,
}

/// `ai` セクションへの mode 別オーバーレイ（未指定キーは「継承」として扱う想定で、現状は保持のみ）。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AiModeOverlay {
	#[serde(default)]
	pub enabled: Option<bool>,
}

/// 通知ポリシーへのオーバーレイ（値の列挙は Mode Manager 側で解釈予定）。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct NotificationsModeOverlay {
	#[serde(default)]
	pub level: Option<String>,
}

/// `[modes.<id>]` 1 ブロック。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RuntimeModeDefinition {
	#[serde(default)]
	pub display_name: Option<String>,
	#[serde(default)]
	pub flowgraph_groups: FlowgraphGroupsModeSpec,
	#[serde(default)]
	pub managed_apps: ManagedAppsModeDirective,
	#[serde(default)]
	pub ai: AiModeOverlay,
	#[serde(default)]
	pub notifications: NotificationsModeOverlay,
}

/// `run_with` の並びと同じ規則で Managed App ID を解決する（`managed_app::build_specs` と一致させる）。
fn collect_resolved_managed_app_ids(run_with: &[RunWith]) -> HashSet<String> {
	let mut used_ids = HashSet::<String>::new();
	let mut out = HashSet::new();
	for (idx, rw) in run_with.iter().enumerate() {
		let auto_id = format!("run-with-{}", idx + 1);
		let id = match rw.explicit_id() {
			Some(id_str) => {
				if !used_ids.insert(id_str.to_string()) {
					log::warn!(
						"《RuntimeMode》 `run_with` の id={:?} が重複しています（Managed App と同じ後勝ち規則）。",
						id_str
					);
				}
				id_str.to_string()
			}
			None => {
				used_ids.insert(auto_id.clone());
				auto_id
			}
		};
		out.insert(id);
	}
	out
}

fn non_empty_list_ids<'a>(xs: &'a [String], ctx: &str) -> Result<()> {
	for s in xs {
		if s.trim().is_empty() {
			bail!("`{}` に空文字列の ID が含まれています。", ctx);
		}
	}
	Ok(())
}

/// `Conf` ロード直後に呼ぶ。`modes` が空なら何もしない（機能オフ）。
pub(crate) fn validate_runtime_modes(conf: &Conf) -> Result<()> {
	if conf.modes.is_empty() {
		if let Some(d) = conf.default_runtime_mode.as_ref() {
			if !d.trim().is_empty() {
				bail!(
					"`default_runtime_mode` が {:?} に設定されていますが、`modes` が空です。",
					d
				);
			}
		}
		return Ok(());
	}

	for (mode_id, def) in conf.modes.iter() {
		if mode_id.trim().is_empty() {
			bail!("`modes` に空のモード ID があります。");
		}
		non_empty_list_ids(&def.flowgraph_groups.enable, &format!("modes.{mode_id}.flowgraph_groups.enable"))?;
		non_empty_list_ids(
			&def.flowgraph_groups.disable,
			&format!("modes.{mode_id}.flowgraph_groups.disable"),
		)?;

		let enable: HashSet<&str> = def.flowgraph_groups.enable.iter().map(|s| s.as_str()).collect();
		let disable: HashSet<&str> = def.flowgraph_groups.disable.iter().map(|s| s.as_str()).collect();
		let both: Vec<_> = enable.intersection(&disable).copied().collect();
		if !both.is_empty() {
			bail!(
				"`modes.{0}` の flowgraph_groups で同一グループが enable と disable の両方に含まれています: {1:?}",
				mode_id,
				both
			);
		}

		let ctx = format!("modes.{mode_id}.managed_apps");
		non_empty_list_ids(&def.managed_apps.start, &format!("{ctx}.start"))?;
		non_empty_list_ids(&def.managed_apps.stop, &format!("{ctx}.stop"))?;
		non_empty_list_ids(&def.managed_apps.minimize, &format!("{ctx}.minimize"))?;
		non_empty_list_ids(&def.managed_apps.leave, &format!("{ctx}.leave"))?;

		let app_ids = collect_resolved_managed_app_ids(&conf.run_with);
		for (field, list) in [
			("start", &def.managed_apps.start),
			("stop", &def.managed_apps.stop),
			("minimize", &def.managed_apps.minimize),
			("leave", &def.managed_apps.leave),
		] {
			for app_id in list {
				if !app_ids.contains(app_id) {
					bail!(
						"`{ctx}.{field}` にある Managed App ID {:?} は `run_with` から解決できません（`id` または run-with-<n> と一致させてください）。",
						app_id
					);
				}
			}
		}
	}

	if let Some(d) = conf.default_runtime_mode.as_ref() {
		if d.trim().is_empty() {
			bail!("`default_runtime_mode` が空文字列です。");
		}
		if !conf.modes.contains_key(d) {
			bail!(
				"`default_runtime_mode` が {:?} ですが、対応する `[modes.{}]` がありません。",
				d,
				d
			);
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::conf::Conf;

	#[test]
	fn deserializes_minimal_modes_and_validates_managed_app() {
		let raw = r#"
run_with = [
  { command = "C:\\obs\\obs64.exe", if_not_running = "obs64", id = "obs" },
]

[modes.streaming]
display_name = "Streaming"
flowgraph_groups.enable = ["assistant", "streaming"]
managed_apps.start = ["obs"]
ai.enabled = true
notifications.level = "stream_safe"
"#;
		let conf: Conf = toml::from_str(raw).expect("toml");
		validate_runtime_modes(&conf).expect("valid");
		assert_eq!(conf.modes.get("streaming").unwrap().display_name.as_deref(), Some("Streaming"));
	}

	#[test]
	fn rejects_unknown_managed_app_id() {
		let raw = r#"
run_with = []
[modes.daily]
managed_apps.stop = ["ghost"]
"#;
		let conf: Conf = toml::from_str(raw).unwrap();
		let err = validate_runtime_modes(&conf).unwrap_err();
		assert!(err.to_string().contains("ghost"), "{}", err);
	}

	#[test]
	fn rejects_enable_disable_overlap() {
		let raw = r#"
[modes.x]
flowgraph_groups.enable = ["a", "b"]
flowgraph_groups.disable = ["b"]
"#;
		let conf: Conf = toml::from_str(raw).unwrap();
		assert!(validate_runtime_modes(&conf).is_err());
	}

	#[test]
	fn default_mode_must_exist() {
		let raw = r#"
default_runtime_mode = "nope"
[modes.daily]
"#;
		let conf: Conf = toml::from_str(raw).unwrap();
		assert!(validate_runtime_modes(&conf).is_err());
	}

	#[test]
	fn default_mode_without_modes_table_errors() {
		let raw = r#"default_runtime_mode = "daily""#;
		let conf: Conf = toml::from_str(raw).unwrap();
		assert!(validate_runtime_modes(&conf).is_err());
	}
}
