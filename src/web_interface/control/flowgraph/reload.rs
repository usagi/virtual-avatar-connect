//! Flowgraph ランタイムのディスク再ロードと WS 通知（`FlowgraphReloaded` / `RestartRecommended`）。

use std::path::Path;

use crate::control_events::ControlEvent;
use crate::flowgraph::loader::{Diagnostic, Severity};
use crate::flowgraph::FlowgraphRuntime;
use crate::SharedState;

/// 書き込み・削除後に `FlowgraphRuntime` を再構築して保存し、`ControlEvent::FlowgraphReloaded` を
/// ブロードキャストする。戻り値は (ok, diagnostics) のサマリ。
///
/// ζ-3: 旧ランタイムの worker と bridges をまず停止し、新 runtime 上で bridges を再 spawn する。
/// `flowgraph.ingress.web_input` は actix-web の route が起動時固定なので hot-swap 不可。
/// 差分があれば `ControlEvent::RestartRecommended` で GUI にトースト。
pub(crate) async fn reload_runtime(state: &SharedState, root: &Path) -> (bool, Vec<Diagnostic>) {
	// ζ-3: 新 runtime は worker 付きで spawn する。state_weak / audio_sink は初回と同じ経路。
	let (state_weak, audio_sink, fg, tx, channel_datum_tx, bridge_handles_arc) = {
		let s = state.read().await;
		(
			std::sync::Arc::downgrade(state),
			Some(s.audio_sink.clone()),
			s.flowgraph.clone(),
			s.control_event_tx.clone(),
			s.channel_datum_tx.clone(),
			s.bridge_handles.clone(),
		)
	};

	// 1. 旧 bridges を落とす。新 runtime の worker を立ち上げる前に切ることで、
	//    旧 trigger に死後 send する事故を防ぐ。
	let old_web_input_snapshot = {
		let mut slot = bridge_handles_arc.lock().await;
		let taken = std::mem::replace(&mut *slot, crate::bridges::BridgeHandles::empty());
		let snap = taken.web_input_snapshot.clone();
		taken.finish_all().await;
		snap
	};

	// 2. 旧 runtime の worker を shutdown（RuntimeHandle 経由）。持っていなければ no-op。
	{
		let old_rt = fg.read().await.clone();
		if let Some(rt) = old_rt {
			if let Some(handle) = rt.handle.as_ref() {
				handle.shutdown().await;
			}
		}
	}

	// 3. 新 runtime を worker 付きで立ち上げる。
	let (conf_opt, mode_for_gate, mode_arc, runtime_root, profile_path) = {
		let s = state.read().await;
		let c = s.conf_source_path.as_ref().and_then(|p| crate::conf::Conf::new_noop_probe(p).ok());
		let m = s.runtime_mode_id.read().ok().and_then(|g| g.clone());
		(c, m, s.runtime_mode_id.clone(), s.runtime_paths.root.clone(), s.conf_source_path.clone())
	};
	let mut rt = FlowgraphRuntime::load_and_spawn(
		root,
		state_weak,
		audio_sink,
		conf_opt.as_ref(),
		mode_for_gate.as_deref(),
		Some(mode_arc),
	);
	if let Some(profile_path) = profile_path.as_ref() {
		rt.set_profile_local_state_snapshot_path(&runtime_root, profile_path);
	}
	let (ok, diags) = (rt.ok, rt.diagnostics.clone());
	let error_count = diags.iter().filter(|d| d.severity == Severity::Error).count();
	let warning_count = diags.iter().filter(|d| d.severity == Severity::Warning).count();
	let node_count = rt.node_meta.len();
	let root_str = rt.root_dir.display().to_string().replace('\\', "/");
	*fg.write().await = Some(rt);

	// 4. 新 runtime 上で bridges を再 spawn し、State に差し戻す。
	let new_handles = crate::bridges::spawn_all_from_state(state, &channel_datum_tx).await;
	let new_web_input_snapshot = new_handles.web_input_snapshot.clone();
	{
		let mut slot = bridge_handles_arc.lock().await;
		*slot = new_handles;
	}

	// 5. web_input の差分を検出したら、actix route は hot-swap できない旨を通知する。
	if crate::bridges::web_input_changed(&old_web_input_snapshot, &new_web_input_snapshot) {
		log::warn!(
			"《Flowgraph/Bridges》 web_input endpoints が変化しました（旧={}, 新={}）。actix route は起動時固定のため、反映には本プロセスの再起動が必要です。",
			old_web_input_snapshot.len(),
			new_web_input_snapshot.len()
		);
		let _ = tx.send(ControlEvent::RestartRecommended {
			reason: "web_input endpoints changed after flowgraph reload".into(),
			details: serde_json::json!({
				"old_web_input_count": old_web_input_snapshot.len(),
				"new_web_input_count": new_web_input_snapshot.len(),
			}),
		});
	}

	// 6. FlowgraphReloaded は最後に送る（受信者 0 でも成功扱い）。
	let _ = tx.send(ControlEvent::FlowgraphReloaded {
		root_dir: root_str,
		ok,
		error_count,
		warning_count,
		node_count,
	});

	(ok, diags)
}
