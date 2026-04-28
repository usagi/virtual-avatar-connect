use crate::shutdown::{ShutdownBroker, ShutdownReason};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

const TRAY_ID: &str = "vac-tray";
const MAIN_WINDOW_LABEL: &str = "main";
const MENU_OPEN_GUI: &str = "vac-open-gui";
const MENU_QUIT: &str = "vac-quit";
const APP_TITLE: &str = "Virtual Avatar Connect";
const MAIN_WINDOW_WIDTH: f64 = 1280.0;
const MAIN_WINDOW_HEIGHT: f64 = 820.0;

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
	let open_gui = MenuItem::with_id(app, MENU_OPEN_GUI, "GUI を開く", true, None::<&str>)?;
	let quit = MenuItem::with_id(app, MENU_QUIT, "終了", true, None::<&str>)?;
	let menu = Menu::with_items(app, &[&open_gui, &quit])?;
	let icon = Image::from_bytes(include_bytes!("../../../assets/brand/vac/derived/vac-tray-default-16.png"))?;

	TrayIconBuilder::with_id(TRAY_ID)
		.tooltip(APP_TITLE)
		.icon(icon)
		.menu(&menu)
		.show_menu_on_left_click(false)
		.on_menu_event(move |app, event| match event.id().as_ref() {
			MENU_OPEN_GUI => show_gui(app),
			MENU_QUIT => request_desktop_shutdown(&shutdown),
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

fn request_desktop_shutdown(shutdown: &ShutdownBroker) {
	log::info!("《Desktop》 tray menu から終了を要求しました。");
	shutdown.trigger(ShutdownReason::Desktop);
}

fn setup_main_window(app: &mut tauri::App, gui_url: String) -> anyhow::Result<WebviewWindow> {
	let gui_url = gui_url.parse()?;
	let window = WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::External(gui_url))
		.title(APP_TITLE)
		.icon(Image::from_bytes(include_bytes!(
			"../../../assets/brand/vac/derived/vac-tray-default-32.png"
		))?)?
		.inner_size(MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT)
		.resizable(true)
		.visible(false)
		.build()?;
	Ok(window)
}

fn attach_hide_on_close(window: WebviewWindow, app_handle: tauri::AppHandle) {
	window.on_window_event(move |event| {
		if let WindowEvent::CloseRequested { api, .. } = event {
			api.prevent_close();
			if let Some(window) = app_handle.get_webview_window(MAIN_WINDOW_LABEL) {
				let _ = window.hide();
			}
		}
	});
}

fn show_gui(app: &tauri::AppHandle) {
	if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
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
