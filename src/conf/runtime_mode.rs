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

/// スロット値の正規化（空・空白のみは `None`）。
pub fn normalize_runtime_mode_slot(raw: Option<&str>) -> Option<String> {
	raw.and_then(|s| {
		let t = s.trim();
		if t.is_empty() {
			None
		} else {
			Some(t.to_string())
		}
	})
}

/// `State.runtime_mode_id` が未設定のときは `default_runtime_mode`、それも無ければ空文字。
pub fn effective_runtime_mode_for_conf(conf: &Conf, slot: Option<&str>) -> String {
	normalize_runtime_mode_slot(slot)
		.or_else(|| conf.default_runtime_mode.clone().filter(|d| !d.trim().is_empty()))
		.unwrap_or_default()
}

/// `POST /modes/plan` および dry-run 用の遷移プレビュー。
#[derive(Debug, Clone, Serialize)]
pub struct ModeTransitionPlan {
	pub from_slot: Option<String>,
	pub to_slot: Option<String>,
	pub from_effective_id: String,
	pub to_effective_id: String,
	/// スロットも実効 ID も変化しないとき true。
	pub noop: bool,
	/// 遷移先モード定義に基づく Flowgraph グループ指定（定義が無ければ空）。
	pub target_flowgraph_groups: FlowgraphGroupsModeSpec,
	pub target_managed_apps: ManagedAppsModeDirective,
	/// 遷移元の enable 集合に無かったが、遷移先の enable に含まれるグループ名。
	pub flowgraph_enable_added_vs_from: Vec<String>,
	/// 遷移元の disable 集合に無かったが、遷移先の disable に含まれるグループ名。
	pub flowgraph_disable_added_vs_from: Vec<String>,
}

/// 現在スロットから `target` スロットへ移るときの宣言差分（Managed App の実行は含まない）。
pub fn build_mode_transition_plan(conf: &Conf, from_slot: Option<&str>, to_slot: Option<&str>) -> Result<ModeTransitionPlan, String> {
	let from_slot_norm = normalize_runtime_mode_slot(from_slot);
	let to_slot_norm = normalize_runtime_mode_slot(to_slot);
	if let Some(ref m) = to_slot_norm {
		if !conf.modes.is_empty() && !conf.modes.contains_key(m) {
			return Err(format!("modes に '{m}' が存在しません"));
		}
	}
	let from_effective_id = effective_runtime_mode_for_conf(conf, from_slot_norm.as_deref());
	let to_effective_id = effective_runtime_mode_for_conf(conf, to_slot_norm.as_deref());
	let noop = from_slot_norm == to_slot_norm && from_effective_id == to_effective_id;

	let from_def = conf
		.modes
		.get(from_effective_id.as_str())
		.cloned()
		.unwrap_or_default();
	let to_def = conf
		.modes
		.get(to_effective_id.as_str())
		.cloned()
		.unwrap_or_default();

	let from_en: HashSet<_> = from_def.flowgraph_groups.enable.iter().cloned().collect();
	let from_dis: HashSet<_> = from_def.flowgraph_groups.disable.iter().cloned().collect();
	let to_en: HashSet<_> = to_def.flowgraph_groups.enable.iter().cloned().collect();
	let to_dis: HashSet<_> = to_def.flowgraph_groups.disable.iter().cloned().collect();

	let mut flowgraph_enable_added_vs_from: Vec<String> = to_en.difference(&from_en).cloned().collect();
	flowgraph_enable_added_vs_from.sort();
	let mut flowgraph_disable_added_vs_from: Vec<String> = to_dis.difference(&from_dis).cloned().collect();
	flowgraph_disable_added_vs_from.sort();

	Ok(ModeTransitionPlan {
		from_slot: from_slot_norm,
		to_slot: to_slot_norm,
		from_effective_id,
		to_effective_id,
		noop,
		target_flowgraph_groups: to_def.flowgraph_groups,
		target_managed_apps: to_def.managed_apps,
		flowgraph_enable_added_vs_from,
		flowgraph_disable_added_vs_from,
	})
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

	#[test]
	fn transition_plan_diffs_enable_disable() {
		let raw = r#"
default_runtime_mode = "daily"
[modes.daily]
flowgraph_groups.enable = ["a"]
flowgraph_groups.disable = []

[modes.streaming]
flowgraph_groups.enable = ["a", "b"]
flowgraph_groups.disable = ["c"]
"#;
		let conf: Conf = toml::from_str(raw).unwrap();
		validate_runtime_modes(&conf).unwrap();
		let p = build_mode_transition_plan(&conf, None, Some("streaming")).unwrap();
		assert_eq!(p.from_effective_id, "daily");
		assert_eq!(p.to_effective_id, "streaming");
		assert!(!p.noop);
		assert_eq!(p.flowgraph_enable_added_vs_from, vec!["b".to_string()]);
		assert_eq!(p.flowgraph_disable_added_vs_from, vec!["c".to_string()]);
	}

	#[test]
	fn transition_plan_rejects_unknown_target_when_modes_nonempty() {
		let raw = r#"
[modes.daily]
"#;
		let conf: Conf = toml::from_str(raw).unwrap();
		validate_runtime_modes(&conf).unwrap();
		assert!(build_mode_transition_plan(&conf, None, Some("ghost")).is_err());
	}
}
