use crate::state::SharedState;
use crate::{bridges, managed_app};

pub(super) async fn stop_managed_apps(state: &SharedState) {
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

pub(super) async fn finish_bridge_handles(state: &SharedState) {
	let handles_arc = state.read().await.bridge_handles.clone();
	let mut slot = handles_arc.lock().await;
	let taken = std::mem::replace(&mut *slot, bridges::BridgeHandles::empty());
	taken.finish_all().await;
}

pub(super) async fn stop_libretranslate(state: &SharedState) {
	let s = state.read().await;
	let fut = async {
		s.libretranslate.lock().await.stop().await;
	};
	if tokio::time::timeout(std::time::Duration::from_secs(5), fut).await.is_err() {
		log::warn!("《Shutdown》 LibreTranslate.stop() が 5 秒以内に完了しませんでした。続行します。");
	}
}
