#[cfg(target_os = "windows")]
pub fn run() -> crate::Result<()> {
	windows_tray::run()
}

#[cfg(not(target_os = "windows"))]
pub fn run() -> crate::Result<()> {
	let runtime = tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()
		.map_err(anyhow::Error::from)?;
	runtime.block_on(crate::run_desktop_headless())
}

#[cfg(target_os = "windows")]
mod windows_tray {
	use crate::shutdown::ShutdownReason;
	use crate::Result;
	use std::sync::Arc;
	use tauri::image::Image;
	use tauri::menu::{Menu, MenuItem};
	use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
	use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

	pub fn run() -> Result<()> {
		let runtime = Arc::new(tokio::runtime::Builder::new_multi_thread()
			.enable_all()
			.build()
			.map_err(anyhow::Error::from)?);

		let core = runtime.block_on(crate::boot_with_standard_bootstrap())?;
		let gui_url = core.gui_url();
		let shutdown = core.shutdown_broker();
		let runtime_for_setup = runtime.clone();
		let runtime_for_after_run = runtime.clone();
		let shutdown_for_setup = shutdown.clone();
		let shutdown_for_serve = shutdown.clone();
		let serve_handle = runtime.spawn(async move {
			let serve_result = core.serve().await;
			let cleanup_result = core.cleanup().await;
			if let Err(e) = serve_result {
				log::error!("《Desktop》 VAC runtime serve がエラー終了しました: {e}");
				shutdown_for_serve.trigger(ShutdownReason::Fatal);
			}
			if let Err(e) = cleanup_result {
				log::error!("《Desktop》 VAC runtime cleanup がエラー終了しました: {e}");
			}
		});

		let app_shutdown = shutdown.clone();
		tauri::Builder::default()
			.setup(move |app| {
				let app_handle = app.handle().clone();
				let open_gui = MenuItem::with_id(app, "vac-open-gui", "GUI を開く", true, None::<&str>)?;
				let quit = MenuItem::with_id(app, "vac-quit", "終了", true, None::<&str>)?;
				let menu = Menu::with_items(app, &[&open_gui, &quit])?;
				let icon = Image::from_bytes(include_bytes!("../resources/icons/vac-tray-default-32.png"))?;
				let shutdown_for_menu = shutdown_for_setup.clone();

				TrayIconBuilder::with_id("vac-tray")
					.tooltip("Virtual Avatar Connect")
					.icon(icon)
					.menu(&menu)
					.show_menu_on_left_click(false)
					.on_menu_event(move |app, event| match event.id().as_ref() {
						"vac-open-gui" => show_gui(app),
						"vac-quit" => shutdown_for_menu.trigger(ShutdownReason::Desktop),
						_ => {}
					})
					.on_tray_icon_event(|tray, event| {
						if let TrayIconEvent::DoubleClick {
							button: MouseButton::Left,
							..
						} = event
						{
							show_gui(tray.app_handle());
						}
					})
					.build(app)?;

				let gui_url = gui_url.parse()?;
				let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(gui_url))
					.title("Virtual Avatar Connect")
					.inner_size(1280.0, 820.0)
					.resizable(true)
					.visible(false)
					.build()?;

				window.on_window_event(move |event| {
					if let WindowEvent::CloseRequested { api, .. } = event {
						api.prevent_close();
						if let Some(window) = app_handle.get_webview_window("main") {
							let _ = window.hide();
						}
					}
				});

				let app_handle_for_shutdown = app.handle().clone();
				runtime_for_setup.spawn(async move {
					app_shutdown.wait().await;
					app_handle_for_shutdown.exit(0);
				});

				log::info!("《Desktop》 Tauri system tray と WebView を初期化しました。");
				Ok(())
			})
			.run(tauri::generate_context!("./tauri.conf.json"))
			.map_err(anyhow::Error::from)?;

		shutdown.trigger(ShutdownReason::Desktop);
		let _ = runtime_for_after_run.block_on(tokio::time::timeout(std::time::Duration::from_secs(10), serve_handle));
		Ok(())
	}

	fn show_gui(app: &tauri::AppHandle) {
		if let Some(window) = app.get_webview_window("main") {
			if let Err(e) = window.show() {
				log::error!("《Desktop》 GUI window を表示できませんでした: {e}");
			}
			if let Err(e) = window.set_focus() {
				log::warn!("《Desktop》 GUI window に focus できませんでした: {e}");
			}
		} else {
			log::error!("《Desktop》 GUI window が見つかりません。");
		}
	}
}
