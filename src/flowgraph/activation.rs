//! RM-3: Runtime Mode に基づく Flowgraph の **exec 経路** の活性フラグ（trigger / 初期ソース / exec 連鎖）。
//!
//! Pure/Stateful の **pull 評価** は止めない（他ファイルからのデータ参照を壊さない）。
//!
//! 正本の意味論: `docs/roadmap/runtime-mode-roadmap.md` §5–6。

use crate::conf::Conf;
use crate::flowgraph::loader::{Diagnostic, DiagnosticCode, FlowgraphFileActivationMeta, LoadedNodeMeta};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// `node_id`（`path/file::local` 形式）からファイル fq を取り出す。
pub fn file_fq_for_node_id(node_id: &str) -> Option<&str> {
	let (a, b) = node_id.split_once("::")?;
	if a.is_empty() || b.is_empty() {
		return None;
	}
	Some(a)
}

/// 1 ファイル分の「この conf / 現在 mode 下で exec を走らせてよいか」。
///
/// `runtime_mode` が `Some`（非空）のときは `conf.default_runtime_mode` より優先。`None` または空文字は conf の default にフォールバック。
pub fn file_effective_exec_active(
	fq: &str,
	meta: &FlowgraphFileActivationMeta,
	conf: &Conf,
	runtime_mode: Option<&str>,
) -> bool {
	let _ = fq;
	if meta.mode_groups.is_empty() {
		return true;
	}
	if conf.modes.is_empty() {
		return meta.default_enabled;
	}
	let mode_id: &str = match runtime_mode.map(str::trim).filter(|s| !s.is_empty()) {
		Some(m) => m,
		None => {
			let Some(m) = conf.default_runtime_mode.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
				return meta.default_enabled;
			};
			m
		}
	};
	let Some(mode_def) = conf.modes.get(mode_id) else {
		return meta.default_enabled;
	};
	let enable = &mode_def.flowgraph_groups.enable;
	let disable = &mode_def.flowgraph_groups.disable;
	let groups = &meta.mode_groups;
	if groups.iter().any(|g| disable.iter().any(|d| d == g)) {
		return false;
	}
	if !enable.is_empty() && !groups.iter().any(|g| enable.iter().any(|e| e == g)) {
		return false;
	}
	true
}

/// 全ノード fq について exec 活性を解決する（欠損キーは [`TriggerGate::is_exec_active`] で true）。
pub fn build_node_exec_active_map(
	conf: &Conf,
	node_meta: &HashMap<String, LoadedNodeMeta>,
	file_activation: &HashMap<String, FlowgraphFileActivationMeta>,
	runtime_mode: Option<&str>,
) -> HashMap<String, bool> {
	let mut out = HashMap::with_capacity(node_meta.len());
	for node_id in node_meta.keys() {
		let active = if let Some(fq) = file_fq_for_node_id(node_id) {
			let meta = file_activation.get(fq).cloned().unwrap_or_default();
			file_effective_exec_active(fq, &meta, conf, runtime_mode)
		} else {
			true
		};
		out.insert(node_id.clone(), active);
	}
	out
}

/// `conf` に `[modes.*]` があるとき、Flowgraph 側 `mode_groups` のうち **どの mode 定義の enable/disable にも出てこない** 名前へ警告を生成する。
pub fn mode_group_orphan_diagnostics(
	conf: &Conf,
	file_activation: &HashMap<String, FlowgraphFileActivationMeta>,
) -> Vec<Diagnostic> {
	if conf.modes.is_empty() {
		return Vec::new();
	}
	let mut referenced: HashSet<String> = HashSet::new();
	for def in conf.modes.values() {
		for g in &def.flowgraph_groups.enable {
			if !g.trim().is_empty() {
				referenced.insert(g.clone());
			}
		}
		for g in &def.flowgraph_groups.disable {
			if !g.trim().is_empty() {
				referenced.insert(g.clone());
			}
		}
	}
	let mut out = Vec::new();
	for (fq, meta) in file_activation {
		for g in &meta.mode_groups {
			if g.trim().is_empty() {
				continue;
			}
			if !referenced.contains(g) {
				out.push(
					Diagnostic::warning(
						DiagnosticCode::OrphanModeGroup,
						format!(
							"Flowgraph「{fq}」の [meta].mode_groups にあるグループ名 '{g}' は、conf のいずれの [modes].flowgraph_groups (enable/disable) にも出現しません（mode 切替で使われません）。"
						),
					)
					.with_hint(fq.as_str().to_string()),
				);
			}
		}
	}
	out
}

/// ワーカーが共有参照する exec ゲート。
#[derive(Debug)]
pub struct TriggerGate {
	node_exec_active: RwLock<HashMap<String, bool>>,
	/// RM-5 quiesce: `true` の間は RM-3 のマップに関わらず **すべてのノード**で exec を抑止する。
	/// Runtime Mode 遷移（Managed App の stop 等）の短時間ウィンドウ用。
	global_exec_suppress: AtomicBool,
}

impl TriggerGate {
	pub fn new(
		conf: &Conf,
		node_meta: &HashMap<String, LoadedNodeMeta>,
		file_activation: &HashMap<String, FlowgraphFileActivationMeta>,
		runtime_mode: Option<&str>,
	) -> Arc<Self> {
		let map = build_node_exec_active_map(conf, node_meta, file_activation, runtime_mode);
		Arc::new(Self {
			node_exec_active: RwLock::new(map),
			global_exec_suppress: AtomicBool::new(false),
		})
	}

	pub fn recompute(
		&self,
		conf: &Conf,
		runtime_mode: Option<&str>,
		node_meta: &HashMap<String, LoadedNodeMeta>,
		file_activation: &HashMap<String, FlowgraphFileActivationMeta>,
	) {
		let map = build_node_exec_active_map(conf, node_meta, file_activation, runtime_mode);
		if let Ok(mut w) = self.node_exec_active.write() {
			*w = map;
		}
	}

	/// `conf` が無い経路（reload 失敗など）では全ノード exec 許可。
	pub fn all_exec_active() -> Arc<Self> {
		Arc::new(Self {
			node_exec_active: RwLock::new(HashMap::new()),
			global_exec_suppress: AtomicBool::new(false),
		})
	}

	/// Mode 遷移などで ingress からの **新規** exec を一時的に止める。必ず `false` に戻すこと。
	pub fn set_global_exec_suppress(&self, v: bool) {
		self.global_exec_suppress.store(v, Ordering::Release);
	}

	pub fn is_exec_active(&self, node_id: &str) -> bool {
		if self.global_exec_suppress.load(Ordering::Acquire) {
			return false;
		}
		self.node_exec_active
			.read()
			.ok()
			.and_then(|g| g.get(node_id).copied())
			.unwrap_or(true)
	}

	/// Control API 診断用: exec が抑止されているノード ID。
	pub fn inactive_node_ids(&self) -> Vec<String> {
		self.node_exec_active
			.read()
			.ok()
			.map(|g| {
				g.iter()
					.filter_map(|(id, active)| if *active { None } else { Some(id.clone()) })
					.collect()
			})
			.unwrap_or_default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::conf::Conf;

	fn meta_groups(groups: &[&str]) -> FlowgraphFileActivationMeta {
		FlowgraphFileActivationMeta {
			mode_groups: groups.iter().map(|s| (*s).to_string()).collect(),
			default_enabled: true,
		}
	}

	#[test]
	fn empty_mode_groups_always_active() {
		let conf: Conf = toml::from_str(
			r#"
			workers = 1
			default_runtime_mode = "daily"
			[modes.daily]
			flowgraph_groups.enable = ["x"]
		"#,
		)
		.unwrap();
		let m = FlowgraphFileActivationMeta::default();
		assert!(file_effective_exec_active("f", &m, &conf, None));
	}

	#[test]
	fn no_conf_modes_uses_default_enabled() {
		let conf: Conf = toml::from_str("workers = 1").unwrap();
		let mut m = meta_groups(&["g"]);
		m.default_enabled = false;
		assert!(!file_effective_exec_active("f", &m, &conf, None));
	}

	#[test]
	fn disable_list_inactive() {
		let conf: Conf = toml::from_str(
			r#"
			workers = 1
			default_runtime_mode = "m"
			[modes.m]
			flowgraph_groups.disable = ["g"]
		"#,
		)
		.unwrap();
		let m = meta_groups(&["g"]);
		assert!(!file_effective_exec_active("f", &m, &conf, None));
	}

	#[test]
	fn enable_list_restricts() {
		let conf: Conf = toml::from_str(
			r#"
			workers = 1
			default_runtime_mode = "m"
			[modes.m]
			flowgraph_groups.enable = ["a"]
		"#,
		)
		.unwrap();
		assert!(!file_effective_exec_active("f", &meta_groups(&["b"]), &conf, None));
		assert!(file_effective_exec_active("f", &meta_groups(&["a"]), &conf, None));
	}

	#[test]
	fn runtime_mode_override_ignores_default() {
		let conf: Conf = toml::from_str(
			r#"
			workers = 1
			default_runtime_mode = "m1"
			[modes.m1]
			flowgraph_groups.enable = ["x"]
			[modes.m2]
			flowgraph_groups.enable = ["y"]
		"#,
		)
		.unwrap();
		let meta = meta_groups(&["y"]);
		assert!(!file_effective_exec_active("f", &meta, &conf, None));
		assert!(file_effective_exec_active("f", &meta, &conf, Some("m2")));
	}

	#[test]
	fn global_exec_suppress_blocks_all_nodes() {
		let conf: Conf = toml::from_str("workers = 1").unwrap();
		let mut node_meta = HashMap::new();
		node_meta.insert(
			"a::n".into(),
			LoadedNodeMeta {
				feature: "flowgraph.util.log".into(),
				file: std::path::PathBuf::from("a"),
				position: None,
				properties: Default::default(),
			},
		);
		let gate = TriggerGate::new(&conf, &node_meta, &HashMap::new(), None);
		assert!(gate.is_exec_active("a::n"));
		gate.set_global_exec_suppress(true);
		assert!(!gate.is_exec_active("a::n"));
		gate.set_global_exec_suppress(false);
		assert!(gate.is_exec_active("a::n"));
	}

	#[test]
	fn orphan_mode_group_diagnostic() {
		let conf: Conf = toml::from_str(
			r#"
			workers = 1
			[modes.a]
			flowgraph_groups.enable = ["used"]
		"#,
		)
		.unwrap();
		let mut fa = std::collections::HashMap::new();
		fa.insert(
			"sub/main".into(),
			FlowgraphFileActivationMeta {
				mode_groups: vec!["orphan".into(), "used".into()],
				default_enabled: true,
			},
		);
		let diags = mode_group_orphan_diagnostics(&conf, &fa);
		assert!(diags.iter().any(|d| d.code == DiagnosticCode::OrphanModeGroup));
	}
}
