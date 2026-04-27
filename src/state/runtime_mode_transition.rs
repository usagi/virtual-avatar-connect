//! RM-5: Runtime Mode 遷移の再入防止・Managed App 適用・Flowgraph フック。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::conf::Conf;
use crate::control_events::ControlEvent;
use crate::flowgraph::activation::TriggerGate;
use crate::flowgraph::node::{PortDirection, TriggerEvent};
use crate::flowgraph::socket::SocketValue;
use crate::SharedState;

use super::apply_runtime_mode_change;
use super::managed_apps_mode_apply::apply_managed_apps_mode_directive;
use super::runtime_mode_apply::{AppliedRuntimeMode, ApplyRuntimeModeError};

/// 遷移処理が走っている間に再度遷移を拒否するためのフラグ。`Drop` で必ず解除する。
pub struct TransitionBusyGuard {
	flag: Arc<AtomicBool>,
}

impl Drop for TransitionBusyGuard {
	fn drop(&mut self) {
		self.flag.store(false, Ordering::Release);
	}
}

/// 遷移ロックの取得に成功したときだけ `Some`。既に他タスクが遷移中なら `None`（HTTP 409 等）。
pub async fn try_begin_runtime_mode_transition(state: &SharedState) -> Option<TransitionBusyGuard> {
	let flag = state.read().await.runtime_mode_transition_busy.clone();
	match flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire) {
		Ok(_) => Some(TransitionBusyGuard { flag }),
		Err(_) => None,
	}
}

/// `apply_runtime_mode_change` に続く follow-up（Managed App 適用結果）をまとめた戻り値。
#[derive(Debug, Clone)]
pub struct RuntimeModeTransitionOutcome {
	pub applied: AppliedRuntimeMode,
	pub managed_reports: Vec<crate::control_events::RuntimeModeManagedAppOp>,
}

fn first_internal_trigger_exec_port(spec: &crate::flowgraph::node::NodeSpec) -> String {
	if let Some(p) = spec
		.inputs
		.iter()
		.find(|p| p.direction == PortDirection::Input && p.is_exec && p.name == "__trigger__")
	{
		return p.name.clone();
	}
	spec.inputs
		.iter()
		.find(|p| p.direction == PortDirection::Input && p.is_exec)
		.map(|p| p.name.clone())
		.unwrap_or_else(|| "__trigger__".into())
}

async fn fire_runtime_mode_changed_flowgraph_hook(state: &SharedState, conf: &Conf, applied: &AppliedRuntimeMode) {
	let node_id = match conf
		.flowgraph_config
		.as_ref()
		.and_then(|c| c.runtime_mode_changed_trigger_node_id.as_deref())
	{
		Some(s) => {
			let t = s.trim();
			if t.is_empty() {
				return;
			}
			t.to_string()
		}
		None => return,
	};
	let (trigger_opt, meta_opt) = {
		let s = state.read().await;
		let fg = s.flowgraph.read().await;
		let Some(rt) = fg.as_ref() else {
			return;
		};
		(rt.trigger(), rt.node_meta.get(&node_id).cloned())
	};
	let Some(trigger) = trigger_opt else {
		log::trace!("《RuntimeMode》 flowgraph hook: worker なし（trigger 無し）");
		return;
	};
	let Some(meta) = meta_opt else {
		log::warn!(
			"《RuntimeMode》 flowgraph hook: node_id={:?} がロード済み node_meta に無いためスキップ",
			node_id
		);
		return;
	};
	let reg = crate::flowgraph::registry::registry();
	let Some(spec) = reg.spec(&meta.feature) else {
		return;
	};
	let exec_port = first_internal_trigger_exec_port(&spec);
	let json = serde_json::json!({
		"previous_slot": applied.previous_slot,
		"current_slot": applied.mode_slot,
		"previous_effective_id": applied.previous_effective_id,
		"current_effective_id": applied.current_effective_id,
	});
	let json_str = json.to_string();
	let ev = TriggerEvent::new(&node_id)
		.with_exec(exec_port)
		.with_override("__content__", SocketValue::String(json_str))
		.with_override(
			"__source_actor__",
			SocketValue::String("virtual-avatar-connect".into()),
		)
		.with_override("__source_kind__", SocketValue::String("runtime_mode_changed".into()))
		.with_override("__meta__", SocketValue::Json(json));
	if let Err(e) = trigger.send(ev) {
		log::warn!("《RuntimeMode》 flowgraph hook: TriggerEvent 送信失敗: {e}");
	}
}

struct GlobalExecSuppressGuard(Arc<TriggerGate>);

impl Drop for GlobalExecSuppressGuard {
	fn drop(&mut self) {
		self.0.set_global_exec_suppress(false);
	}
}

/// スロット更新・`RuntimeModeChanged` WS に加え、`[modes]` があるときは Managed App 宣言の適用と
/// （設定されていれば）Flowgraph への内部 trigger を行う。
///
/// 実効 ID が変わる遷移では、スロット更新の直前から Managed App 適用まで
/// [`TriggerGate::set_global_exec_suppress`] を立て、ingress 等の exec を止める（フック直前に解除）。
pub async fn apply_runtime_mode_transition_full(
	state: &SharedState,
	conf: &Conf,
	normalized: Option<String>,
	reason: Option<String>,
) -> Result<RuntimeModeTransitionOutcome, ApplyRuntimeModeError> {
	let current_slot = {
		let s = state.read().await;
		s.runtime_mode_id.read().ok().and_then(|g| g.clone())
	};
	let will_mutate = match crate::conf::build_mode_transition_plan(conf, current_slot.as_deref(), normalized.as_deref()) {
		Ok(p) => !p.noop,
		Err(msg) => return Err(ApplyRuntimeModeError::PlanFailed(msg)),
	};

	let gate_for_guard = {
		let s = state.read().await;
		let fg = s.flowgraph.read().await;
		fg.as_ref().and_then(|rt| rt.trigger_gate.clone())
	};

	let _global_suppress = if will_mutate {
		gate_for_guard.map(|g| {
			g.set_global_exec_suppress(true);
			log::debug!("《RuntimeMode》 TriggerGate global_exec_suppress=ON（遷移開始）");
			GlobalExecSuppressGuard(g)
		})
	} else {
		None
	};

	let applied = apply_runtime_mode_change(state, conf, normalized, reason).await?;
	let mut managed_reports = Vec::new();

	if !applied.noop {
		if !conf.modes.is_empty() {
			let def = conf
				.modes
				.get(applied.current_effective_id.as_str())
				.cloned()
				.unwrap_or_default();
			managed_reports = apply_managed_apps_mode_directive(state, &def.managed_apps).await;
			if !managed_reports.is_empty() {
				let tx = state.read().await.control_event_tx.clone();
				let _ = tx.send(ControlEvent::RuntimeModeManagedApps {
					previous_effective_id: applied.previous_effective_id.clone(),
					current_effective_id: applied.current_effective_id.clone(),
					ops: managed_reports.clone(),
				});
			}
		}
	}

	drop(_global_suppress);
	if will_mutate {
		log::debug!("《RuntimeMode》 TriggerGate global_exec_suppress=OFF（遷移フック手前）");
	}

	if !applied.noop {
		fire_runtime_mode_changed_flowgraph_hook(state, conf, &applied).await;
	}

	Ok(RuntimeModeTransitionOutcome {
		applied,
		managed_reports,
	})
}
