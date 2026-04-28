use crate::shutdown::{ShutdownBroker, ShutdownReason};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

pub(super) fn setup_desktop_ui(
	app: &mut tauri::App,
	gui_url: String,
	shutdown: Arc<ShutdownBroker>,
	app_handle_tx: tokio::sync::oneshot::Sender<tauri::AppHandle>,
) -> anyhow::Result<()> {
	let app_handle = app.handle().clone();
	let _ = app_handle_tx.send(app.handle().clone());
	setup_tray(app, shutdown)?;
	let window = setup_main_window(app, gui_url)?;
	attach_hide_on_close(window, app_handle);
	log::info!("《Desktop》 Tauri system tray と WebView を初期化しました。");
	Ok(())
}

fn setup_tray(app: &mut tauri::App, shutdown: Arc<ShutdownBroker>) -> tauri::Result<()> {
	let open_gui = MenuItem::with_id(app, "vac-open-gui", "GUI を開く", true, None::<&str>)?;
	let quit = MenuItem::with_id(app, "vac-quit", "終了", true, None::<&str>)?;
	let menu = Menu::with_items(app, &[&open_gui, &quit])?;
	let icon = Image::from_bytes(include_bytes!("../../../assets/brand/vac/derived/vac-tray-default-16.png"))?;

	TrayIconBuilder::with_id("vac-tray")
		.tooltip("Virtual Avatar Connect")
		.icon(icon)
		.menu(&menu)
		.show_menu_on_left_click(false)
		.on_menu_event(move |app, event| match event.id().as_ref() {
			"vac-open-gui" => show_gui(app),
			"vac-quit" => shutdown.trigger(ShutdownReason::Desktop),
			_ => {}
		})
		.on_tray_icon_event(|tray, event| {
			if let TrayIconEvent::DoubleClick {
				button: MouseButton::Left, ..
			} = event
			{
				show_gui(tray.app_handle());
			}
		})
		.build(app)?;
	Ok(())
}

fn setup_main_window(app: &mut tauri::App, gui_url: String) -> anyhow::Result<WebviewWindow> {
	let gui_url = gui_url.parse()?;
	let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(gui_url))
		.title("Virtual Avatar Connect")
		.icon(Image::from_bytes(include_bytes!(
			"../../../assets/brand/vac/derived/vac-tray-default-32.png"
		))?)?
		.inner_size(1280.0, 820.0)
		.resizable(true)
		.visible(false)
		.build()?;
	Ok(window)
}

fn attach_hide_on_close(window: WebviewWindow, app_handle: tauri::AppHandle) {
	window.on_window_event(move |event| {
		if let WindowEvent::CloseRequested { api, .. } = event {
			api.prevent_close();
			if let Some(window) = app_handle.get_webview_window("main") {
				let _ = window.hide();
			}
		}
	});
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
