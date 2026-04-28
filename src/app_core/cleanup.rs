use super::AppCoreParts;
use crate::{bridges, managed_app, Result};

pub(super) async fn cleanup(parts: AppCoreParts) -> Result<()> {
	let managed_stop = {
		let s = parts.state.read().await;
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

	parts.motion_handles.finish_all().await;

	{
		let handles_arc = parts.state.read().await.bridge_handles.clone();
		let mut slot = handles_arc.lock().await;
		let taken = std::mem::replace(&mut *slot, bridges::BridgeHandles::empty());
		taken.finish_all().await;
	}
	for h in parts.ingress_handles.eventsub {
		h.abort();
	}
	for h in parts.ai_handles {
		h.abort();
	}

	{
		let s = parts.state.read().await;
		let fut = async {
			s.libretranslate.lock().await.stop().await;
		};
		if tokio::time::timeout(std::time::Duration::from_secs(5), fut).await.is_err() {
			log::warn!("《Shutdown》 LibreTranslate.stop() が 5 秒以内に完了しませんでした。続行します。");
		}
	}

	log::info!("《Shutdown》 cleanup 完了。プロセスを終了します。");
	Ok(())
}
