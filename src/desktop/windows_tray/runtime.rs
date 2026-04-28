use crate::app_core::AppCore;
use crate::shutdown::{ShutdownBroker, ShutdownReason};
use crate::Result;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub(super) fn build_tokio_runtime() -> Result<Arc<tokio::runtime::Runtime>> {
	Ok(Arc::new(
		tokio::runtime::Builder::new_multi_thread()
			.enable_all()
			.build()
			.map_err(anyhow::Error::from)?,
	))
}

pub(super) fn spawn_vac_runtime_task(
	runtime: &Arc<tokio::runtime::Runtime>,
	core: AppCore,
	shutdown: Arc<ShutdownBroker>,
	app_handle_rx: tokio::sync::oneshot::Receiver<tauri::AppHandle>,
) -> JoinHandle<()> {
	runtime.spawn(async move {
		let app_handle = app_handle_rx.await.ok();
		let (serve_result, cleanup_result) = core.run().await.into_parts();
		if let Err(e) = serve_result {
			log::error!("《Desktop》 VAC runtime serve がエラー終了しました: {e}");
			shutdown.trigger(ShutdownReason::Fatal);
		}
		if let Err(e) = cleanup_result {
			log::error!("《Desktop》 VAC runtime cleanup がエラー終了しました: {e}");
		}
		if let Some(app_handle) = app_handle {
			app_handle.exit(0);
		}
	})
}
