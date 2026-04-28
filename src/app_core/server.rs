use super::AppCoreParts;
use crate::{shutdown, Result};
use actix_web::dev::ServerHandle;
use http::build_http_server;
use runtime::ServerRuntime;
use std::sync::Arc;

mod http;
mod runtime;

pub(super) async fn run_services(parts: &AppCoreParts) -> Result<()> {
	let runtime = ServerRuntime::from_app_core(parts);
	let shutdown = runtime.shutdown.clone();
	let server = build_http_server(runtime)?;

	spawn_http_shutdown_watcher(server.handle(), shutdown);

	server.await?;
	log::info!("《Shutdown》 actix HTTP サーバーが停止しました。");
	Ok(())
}

fn spawn_http_shutdown_watcher(server_handle: ServerHandle, shutdown: Arc<shutdown::ShutdownBroker>) {
	tokio::spawn(async move {
		shutdown.wait().await;
		log::info!("《Shutdown》 actix HTTP サーバーへの graceful stop を要求します。");
		server_handle.stop(true).await;
	});
}
