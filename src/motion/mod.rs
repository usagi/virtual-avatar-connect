//! Phase M0: **motion 層** — VMC 生 UDP パススルー（Flowgraph 非依存）。
//!
//! 設計の正本: [`docs/roadmap/v2-vmc-and-restructure.md`](../../docs/roadmap/v2-vmc-and-restructure.md)  
//! M0 詳細: [`docs/roadmap/phase-mu-vmc-motion-m0.md`](../../docs/roadmap/phase-mu-vmc-motion-m0.md)
//!
//! ## Crate 分割時の依存契約（Step 4）
//!
//! root crate 側は **`crate::conf`** と **`crate::shutdown`**（および `vac-motion`）のみを参照する。
//! **`flowgraph` / `bridges` / `state` / `web_interface` へは依存しない**（VMC の Flowgraph 入口は `bridges::vmc_ingress`）。

mod osc;
mod vmc_raw;

pub use vac_motion::{parse_vmc_payload, MotionFrame};
#[cfg(test)]
pub use vac_motion::OscMessageWire;

use crate::conf::Conf;
use crate::conf::VmcPassthroughSpec;
use crate::shutdown::ShutdownBroker;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;

fn warn_duplicate_vmcbinds(specs: &[VmcPassthroughSpec]) {
	let mut counts: HashMap<SocketAddr, usize> = HashMap::new();
	for spec in specs {
		if !spec.enabled || spec.forward_to.is_empty() {
			continue;
		}
		if let Ok(a) = spec.bind.trim().parse::<SocketAddr>() {
			*counts.entry(a).or_insert(0) += 1;
		}
	}
	for (addr, n) in counts.iter().filter(|(_, c)| **c > 1) {
		let labels: Vec<&str> = specs
			.iter()
			.filter(|s| s.enabled && !s.forward_to.is_empty())
			.filter(|s| s.bind.trim().parse::<SocketAddr>().ok().as_ref() == Some(addr))
			.map(|s| {
				s.label
					.as_deref()
					.map(str::trim)
					.filter(|t| !t.is_empty())
					.unwrap_or("(label なし)")
			})
			.collect();
		log::warn!(
			"《Motion/VMC》 同一 bind {} を {} 件の vmc_passthrough が使用しています（識別子: {}）。先着のみ bind に成功しうるため、意図しない重複なら label / bind を見直してください。",
			addr,
			n,
			labels.join(", ")
		);
	}
}

/// `enabled` かつ `forward_to` 非空かつ `bind` がパース可能なエントリについて、同一 `bind` が 2 回以上現れたアドレスを返す（テスト用）。
#[cfg(test)]
pub(crate) fn duplicate_vmc_bind_addrs_for_test(specs: &[VmcPassthroughSpec]) -> Vec<SocketAddr> {
	let mut counts: HashMap<SocketAddr, usize> = HashMap::new();
	for spec in specs {
		if !spec.enabled || spec.forward_to.is_empty() {
			continue;
		}
		if let Ok(a) = spec.bind.trim().parse::<SocketAddr>() {
			*counts.entry(a).or_insert(0) += 1;
		}
	}
	let mut dups: Vec<SocketAddr> = counts.into_iter().filter(|(_, c)| *c > 1).map(|(a, _)| a).collect();
	dups.sort_by_key(|a| (a.ip(), a.port()));
	dups
}

/// 起動中の motion タスク。`run()` の cleanup で [`MotionHandles::finish_all`] する。
pub struct MotionHandles {
	tasks: Vec<JoinHandle<()>>,
}

impl MotionHandles {
	pub fn empty() -> Self {
		Self { tasks: Vec::new() }
	}

	#[allow(dead_code)] // 将来の診断・条件分岐用（現状は spawn 直後のみ参照）
	pub fn is_empty(&self) -> bool {
		self.tasks.is_empty()
	}

	/// `conf.motion` に従い VMC passthrough タスクを spawn する。
	pub fn spawn_all(conf: &Conf, shutdown: Arc<ShutdownBroker>) -> Self {
		let Some(m) = conf.motion.as_ref() else {
			return Self::empty();
		};
		warn_duplicate_vmcbinds(&m.vmc_passthrough);
		let mut tasks = Vec::new();
		for spec in &m.vmc_passthrough {
			if let Some(h) = vmc_raw::try_spawn(spec, shutdown.clone()) {
				tasks.push(h);
			}
		}
		if !tasks.is_empty() {
			log::info!("《Motion》 ワーカー {} 本起動（VMC passthrough）", tasks.len());
		}
		Self { tasks }
	}

	/// 全タスクを中止し、終了を待つ（cleanup 用）。
	pub async fn finish_all(self) {
		for h in self.tasks {
			h.abort();
			let _ = h.await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::conf::VmcPassthroughSpec;

	#[test]
	fn motion_handles_default_empty() {
		let h = MotionHandles::empty();
		assert!(h.is_empty());
	}

	#[test]
	fn duplicate_vmc_bind_addrs_detects_dup() {
		let specs = vec![
			VmcPassthroughSpec {
				enabled: true,
				bind: "0.0.0.0:59991".into(),
				forward_to: vec!["127.0.0.1:2".into()],
				label: None,
			},
			VmcPassthroughSpec {
				enabled: true,
				bind: "0.0.0.0:59991".into(),
				forward_to: vec!["127.0.0.1:3".into()],
				label: Some("b".into()),
			},
		];
		let dups = duplicate_vmc_bind_addrs_for_test(&specs);
		assert_eq!(dups.len(), 1);
		assert_eq!(dups[0], "0.0.0.0:59991".parse().unwrap());
	}

	#[test]
	fn duplicate_vmc_bind_respects_disabled() {
		let specs = vec![
			VmcPassthroughSpec {
				enabled: true,
				bind: "0.0.0.0:59992".into(),
				forward_to: vec!["127.0.0.1:2".into()],
				label: None,
			},
			VmcPassthroughSpec {
				enabled: false,
				bind: "0.0.0.0:59992".into(),
				forward_to: vec!["127.0.0.1:3".into()],
				label: None,
			},
		];
		assert!(duplicate_vmc_bind_addrs_for_test(&specs).is_empty());
	}
}
