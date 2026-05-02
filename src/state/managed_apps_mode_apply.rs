//! RM-5: `[modes.*].managed_apps` の start / stop / minimize をランタイムに適用する。

use std::collections::HashSet;

use crate::conf::ManagedAppsModeDirective;
use crate::control_events::RuntimeModeManagedAppOp;
use crate::managed_app::{self, ManagedAppSpec};
use crate::SharedState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
	Stop,
	Start,
	Minimize,
}

fn unique_in_order(xs: &[String]) -> Vec<String> {
	let mut out = Vec::new();
	let mut seen = HashSet::<String>::new();
	for x in xs {
		if seen.insert(x.clone()) {
			out.push(x.clone());
		}
	}
	out
}

/// stop → start → minimize の順。各リスト内の重複のみ除去（同一 ID が stop と start の両方にあれば両方実行）。
fn planned_ops(directive: &ManagedAppsModeDirective) -> Vec<(Phase, String, bool)> {
	let leave: HashSet<_> = directive.leave.iter().cloned().collect();
	let mut out = Vec::new();
	for id in unique_in_order(&directive.stop) {
		let skip = leave.contains(&id);
		out.push((Phase::Stop, id, skip));
	}
	for id in unique_in_order(&directive.start) {
		let skip = leave.contains(&id);
		out.push((Phase::Start, id, skip));
	}
	for id in unique_in_order(&directive.minimize) {
		let skip = leave.contains(&id);
		out.push((Phase::Minimize, id, skip));
	}
	out
}

async fn resolve_spec_status(state: &SharedState, id: &str) -> Option<(ManagedAppSpec, crate::managed_app::ManagedAppStatus)> {
	let registry = state.read().await.managed_apps.clone();
	let r = registry.read().await;
	let spec = r.find_spec(id).cloned()?;
	let st = r
		.statuses
		.get(id)
		.cloned()
		.unwrap_or_else(|| crate::managed_app::ManagedAppStatus::unknown(id));
	Some((spec, st))
}

async fn one_op(state: &SharedState, phase: Phase, id: &str, skip_leave: bool) -> RuntimeModeManagedAppOp {
	if skip_leave {
		return RuntimeModeManagedAppOp {
			id: id.to_string(),
			op: "skip_leave".to_string(),
			ok: true,
			detail: None,
		};
	}
	match phase {
		Phase::Stop => {
			let Some((spec, status)) = resolve_spec_status(state, id).await else {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "stop".to_string(),
					ok: false,
					detail: Some("not_found".into()),
				};
			};
			if !spec.supports_status {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "stop".to_string(),
					ok: false,
					detail: Some("stop_requires_status".into()),
				};
			}
			if !status.running {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "stop".to_string(),
					ok: true,
					detail: Some("not_running".into()),
				};
			}
			let cfg = spec.shutdown.clone();
			match managed_app::stop_entry_graceful(&status.pids, cfg).await {
				Ok(out) => RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "stop".to_string(),
					ok: true,
					detail: Some(format!("closed={} terminated={}", out.closed_windows, out.terminated_pids)),
				},
				Err(e) => RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "stop".to_string(),
					ok: false,
					detail: Some(e.to_string()),
				},
			}
		}
		Phase::Start => {
			let Some((spec, status)) = resolve_spec_status(state, id).await else {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "start".to_string(),
					ok: false,
					detail: Some("not_found".into()),
				};
			};
			if status.running {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "start".to_string(),
					ok: true,
					detail: Some("already_running".into()),
				};
			}
			let rw = crate::conf::RunWith::CommandIfProcessIsNotRunning {
				command: spec.command.clone(),
				if_not_running: spec.process_marker.clone(),
				run_as_admin: Some(spec.run_as_admin),
				working_dir: spec.working_dir.clone(),
				minimized: Some(spec.minimized),
				id: Some(spec.id.clone()),
				label: Some(spec.label.clone()),
				shutdown: None,
			};
			match managed_app::start_entry(&rw) {
				Ok(()) => RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "start".to_string(),
					ok: true,
					detail: None,
				},
				Err(e) => RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "start".to_string(),
					ok: false,
					detail: Some(e.to_string()),
				},
			}
		}
		Phase::Minimize => {
			let Some((spec, status)) = resolve_spec_status(state, id).await else {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "minimize".to_string(),
					ok: false,
					detail: Some("not_found".into()),
				};
			};
			if !spec.supports_status {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "minimize".to_string(),
					ok: false,
					detail: Some("minimize_requires_status".into()),
				};
			}
			if !status.running {
				return RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "minimize".to_string(),
					ok: false,
					detail: Some("not_running".into()),
				};
			}
			#[cfg(target_os = "windows")]
			{
				let n = managed_app::minimize_pids(&status.pids);
				RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "minimize".to_string(),
					ok: true,
					detail: Some(format!("scheduled_pids={n}")),
				}
			}
			#[cfg(not(target_os = "windows"))]
			{
				let _ = spec;
				RuntimeModeManagedAppOp {
					id: id.to_string(),
					op: "minimize".to_string(),
					ok: false,
					detail: Some("unsupported_os".into()),
				}
			}
		}
	}
}

/// `directive` を順に適用する（stop → start → minimize、`leave` は `skip_leave` 行として記録）。
pub async fn apply_managed_apps_mode_directive(state: &SharedState, directive: &ManagedAppsModeDirective) -> Vec<RuntimeModeManagedAppOp> {
	let mut reports = Vec::new();
	for (phase, id, skip_leave) in planned_ops(directive) {
		reports.push(one_op(state, phase, &id, skip_leave).await);
	}
	reports
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn planned_ops_stop_before_start_same_id() {
		let d = ManagedAppsModeDirective {
			start: vec!["a".into()],
			stop: vec!["a".into()],
			minimize: vec![],
			leave: vec![],
		};
		let p = planned_ops(&d);
		assert_eq!(p.len(), 2);
		assert_eq!(p[0].0, Phase::Stop);
		assert_eq!(p[1].0, Phase::Start);
	}

	#[test]
	fn planned_ops_leave_skips() {
		let d = ManagedAppsModeDirective {
			start: vec![],
			stop: vec!["x".into()],
			minimize: vec!["x".into()],
			leave: vec!["x".into()],
		};
		let p = planned_ops(&d);
		assert_eq!(p.len(), 2);
		assert!(p[0].2 && p[1].2, "both skip_leave: {p:?}");
	}
}
