use super::AppCoreParts;
use crate::Result;
use http::build_http_server;
use lifecycle::spawn_http_shutdown_watcher;
use runtime::ServerRuntime;

mod http;
mod lifecycle;
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
