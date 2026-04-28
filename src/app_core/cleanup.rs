use super::AppCoreParts;
use crate::state::SharedState;
use crate::{bridges, managed_app, Result};

pub(super) async fn cleanup(parts: AppCoreParts) -> Result<()> {
	let state = parts.state.clone();
	let tasks = parts.tasks;

	stop_managed_apps(&state).await;

	tasks.motion_handles.finish_all().await;

	finish_bridge_handles(&state).await;
	stop_libretranslate(&state).await;
	abort_ingress_handles(tasks.ingress_handles);
	abort_ai_handles(tasks.ai_handles);

	log::info!("《Shutdown》 cleanup 完了。プロセスを終了します。");
	Ok(())
}

async fn stop_managed_apps(state: &SharedState) {
	let managed_stop = {
		let s = state.read().await;
		let registry = s.managed_apps.clone();
		log::info!("《Shutdown》 ManagedApp 全停止を試行します（entry ごとの shutdown cfg を使用）。");
		managed_app::stop_all_graceful(&registry).await
	};
	for (id, outcome) in &managed_stop {
		log::info!(
			"《Shutdown》 ManagedApp 停止結果 id={} closed_windows={} terminated_pids={}",
			id,
			outcome.closed_windows,
			outcome.terminated_pids
		);
	}
}

async fn finish_bridge_handles(state: &SharedState) {
	let handles_arc = state.read().await.bridge_handles.clone();
	let mut slot = handles_arc.lock().await;
	let taken = std::mem::replace(&mut *slot, bridges::BridgeHandles::empty());
	taken.finish_all().await;
}

fn abort_ingress_handles(handles: crate::processor::ingress::IngressHandles) {
	for h in handles.eventsub {
		h.abort();
	}
}

fn abort_ai_handles(handles: Vec<tokio::task::JoinHandle<()>>) {
	for h in handles {
		h.abort();
	}
}

async fn stop_libretranslate(state: &SharedState) {
	let s = state.read().await;
	let fut = async {
		s.libretranslate.lock().await.stop().await;
	};
	if tokio::time::timeout(std::time::Duration::from_secs(5), fut).await.is_err() {
		log::warn!("《Shutdown》 LibreTranslate.stop() が 5 秒以内に完了しませんでした。続行します。");
	}
}
