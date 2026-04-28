use crate::shutdown::ShutdownReason;
use crate::Result;
use runtime::{build_tokio_runtime, spawn_vac_runtime_task};
use ui::setup_desktop_ui;

mod runtime;
mod ui;

const DESKTOP_RUNTIME_SHUTDOWN_WAIT_SECONDS: u64 = 10;

pub fn run() -> Result<()> {
	let runtime = build_tokio_runtime()?;

	let core = runtime.block_on(crate::bootstrap::boot_app_core_with_standard_bootstrap())?;
	let (gui_url, shutdown) = core.runtime_handle().into_parts();
	let runtime_for_after_run = runtime.clone();
	let shutdown_for_setup = shutdown.clone();
	let (app_handle_tx, app_handle_rx) = tokio::sync::oneshot::channel::<tauri::AppHandle>();
	let serve_handle = spawn_vac_runtime_task(&runtime, core, shutdown.clone(), app_handle_rx);

	tauri::Builder::default()
		.setup(move |app| {
			setup_desktop_ui(app, gui_url.clone(), shutdown_for_setup.clone(), app_handle_tx)?;
			Ok(())
		})
		.run(tauri::generate_context!("./tauri.conf.json"))
		.map_err(anyhow::Error::from)?;

	shutdown.trigger(ShutdownReason::Desktop);
	let _ = runtime_for_after_run.block_on(tokio::time::timeout(
		std::time::Duration::from_secs(DESKTOP_RUNTIME_SHUTDOWN_WAIT_SECONDS),
		serve_handle,
	));
	Ok(())
}
