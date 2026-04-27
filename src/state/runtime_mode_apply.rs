//! Runtime Mode の適用。Control API と Flowgraph の `flowgraph.mode.transit` が共有する。

use crate::conf::{effective_runtime_mode_for_conf, Conf};
use crate::control_events::ControlEvent;
use crate::SharedState;

#[derive(Debug)]
pub enum ApplyRuntimeModeError {
	UnknownMode(String),
	/// `build_mode_transition_plan` が拒否したとき（`apply_runtime_mode_transition_full` の先頭）。
	PlanFailed(String),
}

impl std::fmt::Display for ApplyRuntimeModeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownMode(m) => write!(f, "unknown_mode: {m}"),
			Self::PlanFailed(m) => write!(f, "plan_failed: {m}"),
		}
	}
}

impl std::error::Error for ApplyRuntimeModeError {}

/// 適用後のスロットと、WS 通知用の前後実効 ID（呼び出し側が詳細ログに使える）。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AppliedRuntimeMode {
	pub mode_slot: Option<String>,
	pub previous_slot: Option<String>,
	pub previous_effective_id: String,
	pub current_effective_id: String,
	pub noop: bool,
}

/// `runtime_mode_id` を更新し、必要なら TriggerGate を再計算して `RuntimeModeChanged` を送る。
pub async fn apply_runtime_mode_change(
	state: &SharedState,
	conf: &Conf,
	normalized: Option<String>,
	reason: Option<String>,
) -> Result<AppliedRuntimeMode, ApplyRuntimeModeError> {
	if let Some(ref m) = normalized {
		if !conf.modes.is_empty() && !conf.modes.contains_key(m) {
			return Err(ApplyRuntimeModeError::UnknownMode(m.clone()));
		}
	}

	let (tx, previous_slot, previous_effective_id) = {
		let s = state.read().await;
		let prev = s.runtime_mode_id.read().ok().and_then(|g| g.clone());
		let prev_eff = effective_runtime_mode_for_conf(conf, prev.as_deref());
		(s.control_event_tx.clone(), prev, prev_eff)
	};

	{
		let slot = {
			let s = state.read().await;
			s.runtime_mode_id.clone()
		};
		let mut w = slot.write().unwrap_or_else(|e| e.into_inner());
		*w = normalized;
	}

	let mode_slot = {
		let s = state.read().await;
		s.runtime_mode_id.read().ok().and_then(|g| g.clone())
	};
	let current_effective_id = effective_runtime_mode_for_conf(conf, mode_slot.as_deref());
	let noop = previous_slot == mode_slot && previous_effective_id == current_effective_id;

	{
		let s = state.read().await;
		let fg = s.flowgraph.read().await;
		if let Some(rt) = fg.as_ref() {
			rt.recompute_trigger_gate(conf, mode_slot.as_deref());
		}
	}

	if !noop {
		let ev = ControlEvent::RuntimeModeChanged {
			previous_slot: previous_slot.clone(),
			current_slot: mode_slot.clone(),
			previous_effective_id: previous_effective_id.clone(),
			current_effective_id: current_effective_id.clone(),
			reason,
		};
		if let Err(e) = tx.send(ev) {
			log::trace!("《RuntimeMode》 control_event_tx.send(RuntimeModeChanged) に失敗: {e}");
		}
	}

	Ok(AppliedRuntimeMode {
		mode_slot,
		previous_slot,
		previous_effective_id,
		current_effective_id,
		noop,
	})
}
