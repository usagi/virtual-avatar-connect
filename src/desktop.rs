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
	use tao::event::{Event, StartCause};
	use tao::event_loop::{ControlFlow, EventLoopBuilder};
	use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
	use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

	enum DesktopEvent {
		Menu(MenuEvent),
		RuntimeStopped,
	}

	pub fn run() -> Result<()> {
		let mut event_loop_builder = EventLoopBuilder::<DesktopEvent>::with_user_event();
		let event_loop = event_loop_builder.build();
		let event_proxy = event_loop.create_proxy();

		MenuEvent::set_event_handler(Some({
			let proxy = event_proxy.clone();
			move |event| {
				let _ = proxy.send_event(DesktopEvent::Menu(event));
			}
		}));

		let runtime = tokio::runtime::Builder::new_multi_thread()
			.enable_all()
			.build()
			.map_err(anyhow::Error::from)?;

		let core = runtime.block_on(crate::boot_with_standard_bootstrap())?;
		let gui_url = core.gui_url();
		let shutdown = core.shutdown_broker();

		let serve_proxy = event_proxy.clone();
		runtime.spawn(async move {
			let serve_result = core.serve().await;
			let cleanup_result = core.cleanup().await;
			if let Err(e) = serve_result {
				log::error!("《Desktop》 VAC runtime serve がエラー終了しました: {e}");
			}
			if let Err(e) = cleanup_result {
				log::error!("《Desktop》 VAC runtime cleanup がエラー終了しました: {e}");
			}
			let _ = serve_proxy.send_event(DesktopEvent::RuntimeStopped);
		});

		let open_id = MenuId::new("vac-open-gui");
		let quit_id = MenuId::new("vac-quit");
		let mut tray: Option<TrayIcon> = None;

		event_loop.run(move |event, _, control_flow| {
			*control_flow = ControlFlow::Wait;
			let _runtime_guard = &runtime;

			match event {
				Event::NewEvents(StartCause::Init) if tray.is_none() => match build_tray_icon(open_id.clone(), quit_id.clone()) {
					Ok(icon) => {
						tray = Some(icon);
						log::info!("《Desktop》 system tray を初期化しました。");
					}
					Err(e) => {
						log::error!("《Desktop》 system tray の初期化に失敗しました: {e}");
						shutdown.trigger(ShutdownReason::Fatal);
					}
				},
				Event::UserEvent(DesktopEvent::Menu(event)) if event.id == open_id => {
					if let Err(e) = webbrowser::open(&gui_url) {
						log::error!("《Desktop》 GUI を開けませんでした url={gui_url}: {e}");
					}
				}
				Event::UserEvent(DesktopEvent::Menu(event)) if event.id == quit_id => {
					shutdown.trigger(ShutdownReason::ControlApi);
				}
				Event::UserEvent(DesktopEvent::RuntimeStopped) => {
					*control_flow = ControlFlow::Exit;
				}
				_ => {}
			}
		});
	}

	fn build_tray_icon(open_id: MenuId, quit_id: MenuId) -> anyhow::Result<TrayIcon> {
		let menu = Menu::new();
		let open_gui = MenuItem::with_id(open_id, "GUI を開く", true, None);
		let quit = MenuItem::with_id(quit_id, "終了", true, None);
		menu.append_items(&[&open_gui, &quit])?;

		let icon = load_icon()?;
		let tray = TrayIconBuilder::new()
			.with_tooltip("Virtual Avatar Connect")
			.with_icon(icon)
			.with_menu(Box::new(menu))
			.build()?;
		Ok(tray)
	}

	fn load_icon() -> anyhow::Result<Icon> {
		let bytes = include_bytes!("../resources/icons/vac-tray-default-32.png");
		let image = image::load_from_memory(bytes)?.into_rgba8();
		let (width, height) = image.dimensions();
		Ok(Icon::from_rgba(image.into_raw(), width, height)?)
	}
}
