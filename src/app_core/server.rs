use super::AppCoreParts;
use crate::conf::Conf;
use crate::state::SharedState;
use crate::{bridges, flowgraph, shutdown, web_interface, Result};
use actix_files::Files;
use actix_web::web::Data;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn normalize_loopback_address(address: &str) -> String {
	if let Ok(socket) = address.parse::<SocketAddr>() {
		let port = socket.port();
		let ip = socket.ip();
		if ip.is_unspecified() {
			return format!("127.0.0.1:{port}");
		}
		if ip.is_ipv6() {
			return format!("[{ip}]:{port}");
		}
		return socket.to_string();
	}

	address
		.strip_prefix("0.0.0.0:")
		.map(|port| format!("127.0.0.1:{port}"))
		.unwrap_or_else(|| address.to_string())
}

pub(super) async fn run_services(parts: &AppCoreParts) -> Result<()> {
	let runtime = ServerRuntime::from_app_core(parts);
	let shutdown = runtime.shutdown.clone();
	let workers = runtime.workers;
	let web_ui_address = runtime.web_ui_address.clone();
	let server = actix_web::HttpServer::new(move || {
		let runtime = runtime.clone();
		let reg = runtime.web_input_registry.clone();
		let output_root = runtime.output_root.clone();
		let app = actix_web::App::new()
			.wrap(actix_web::middleware::Condition::new(
				runtime.conf.web_ui_compress,
				actix_web::middleware::Compress::default(),
			))
			.app_data(Data::new(runtime.state))
			.app_data(Data::new(web_interface::output::OutputPaths { root: output_root.clone() }))
			.app_data(Data::new(reg.clone()))
			.app_data(Data::new(runtime.control_api_runtime))
			.app_data(Data::new(runtime.flowgraph_trigger.clone()))
			.configure({
				let r = reg.clone();
				move |cfg| {
					web_interface::web_input::register_web_input_routes(cfg, &r);
				}
			})
			.configure({
				let eps = runtime.flowgraph_web_input_endpoints.clone();
				move |cfg| {
					bridges::web_input::register_routes(cfg, eps.as_ref());
				}
			})
			.configure(web_interface::control::register)
			.configure({
				let gui_dist_path = runtime.conf.gui_dist_path.clone();
				move |cfg| {
					web_interface::gui::register(cfg, gui_dist_path.as_deref());
				}
			})
			.service(web_interface::websocket)
			.service(web_interface::input::get_index)
			.service(web_interface::input::get_subfile)
			.service(web_interface::output::post)
			.service(web_interface::output::get_index)
			.service(web_interface::output::get_subfile)
			.service(Files::new("/browser-output", output_root.clone()))
			.service(web_interface::status::get)
			.service(web_interface::favicon);
		if let Some(web_ui_resources_path) = runtime.conf.web_ui_resources_path.clone() {
			app.service(Files::new("/resources", web_ui_resources_path))
		} else {
			app
		}
	})
	.workers(workers)
	.bind(web_ui_address)?
	.disable_signals()
	.shutdown_timeout(2)
	.run();

	let server_handle = server.handle();
	let shutdown_for_server = shutdown.clone();
	tokio::spawn(async move {
		shutdown_for_server.wait().await;
		log::info!("《Shutdown》 actix HTTP サーバーへの graceful stop を要求します。");
		server_handle.stop(true).await;
	});

	server.await?;
	log::info!("《Shutdown》 actix HTTP サーバーが停止しました。");
	Ok(())
}

#[derive(Clone)]
struct ServerRuntime {
	conf: Conf,
	state: SharedState,
	web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
	control_api_runtime: web_interface::control::ControlApiRuntime,
	flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
	flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
	shutdown: Arc<shutdown::ShutdownBroker>,
	output_root: PathBuf,
	workers: usize,
	web_ui_address: String,
}

impl ServerRuntime {
	fn from_app_core(parts: &AppCoreParts) -> Self {
		let conf = parts.conf.clone();
		let output_root = conf
			.browser_source
			.as_ref()
			.and_then(|b| b.document_root.clone())
			.unwrap_or_else(|| PathBuf::from("output"));
		let workers = conf.get_workers();
		let web_ui_address = conf.get_web_ui_address().to_string();
		Self {
			conf,
			state: parts.state.clone(),
			web_input_registry: parts.web_input_registry.clone(),
			control_api_runtime: parts.control_api_runtime.clone(),
			flowgraph_web_input_endpoints: parts.flowgraph_web_input_endpoints.clone(),
			flowgraph_trigger: parts.flowgraph_trigger.clone(),
			shutdown: parts.shutdown.clone(),
			output_root,
			workers,
			web_ui_address,
		}
	}
}
