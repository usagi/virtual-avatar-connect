use crate::shutdown;
use actix_web::dev::ServerHandle;
use std::sync::Arc;

pub(super) fn spawn_http_shutdown_watcher(server_handle: ServerHandle, shutdown: Arc<shutdown::ShutdownBroker>) {
	tokio::spawn(async move {
		shutdown.wait().await;
		log::info!("《Shutdown》 actix HTTP サーバーへの graceful stop を要求します。");
		server_handle.stop(true).await;
	});
}
