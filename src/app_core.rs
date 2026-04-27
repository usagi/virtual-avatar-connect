//! Step 7（再構造化）: 通常運転の **bootstrap + HTTP serve + shutdown cleanup**。
//!
//! `crate::run()` はロガー・CLI 特殊モード・conf ロードまでを担当し、本モジュールが
//! `ShutdownBroker` 以降の常駐ランタイム本体をまとめる。将来 `vac-app` crate へ移す際の境界の目印。

use crate::bridges;
use crate::conf::Conf;
use crate::managed_app;
use crate::motion;
use crate::processor;
use crate::shutdown;
use crate::state::SharedState;
use crate::{ai, flowgraph, web_interface, Result, SharedAudioSink};
use actix_files::Files;
use actix_web::web::Data;
use std::net::SocketAddr;
use std::sync::Arc;

/// conf ロード済み・`run_with` 済みの状態から起動する VAC 常駐ランタイム本体。
///
/// CLI / desktop runner は、最終的にこの `boot` / `serve` / `cleanup`
/// 境界を共有する。現時点では `run_vac_application` が従来通り直列に呼ぶ。
pub struct AppCore {
	conf: Conf,
	state: SharedState,
	shutdown: Arc<shutdown::ShutdownBroker>,
	ai_handles: Vec<tokio::task::JoinHandle<()>>,
	ingress_handles: processor::ingress::IngressHandles,
	motion_handles: motion::MotionHandles,
	web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
	control_api_runtime: web_interface::control::ControlApiRuntime,
	flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
	flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
}

impl AppCore {
	pub async fn boot(conf: Conf, audio_sink: SharedAudioSink) -> Result<Self> {
		let shutdown = shutdown::ShutdownBroker::new();
		shutdown::spawn_ctrl_c_listener(shutdown.clone());

		let state = crate::State::new(&conf, audio_sink, shutdown.clone()).await?;

		let ai_tx = state.read().await.ai_observation_tx.clone();
		let twitch_eventsub_for_ai = conf
			.twitch
			.as_ref()
			.and_then(|t| t.eventsub.as_ref())
			.map(|e| std::sync::Arc::new(e.clone()));
		let twitch_moderator_for_ai = conf
			.twitch
			.as_ref()
			.and_then(|t| t.moderator.as_ref())
			.map(|m| std::sync::Arc::new(m.clone()));
		let twitch_default_broadcaster_login = conf.twitch.as_ref().map(|t| {
			t.eventsub
				.as_ref()
				.and_then(|e| e.broadcaster_login.clone())
				.unwrap_or_else(|| t.username.clone())
		});
		let ai_handles = ai::spawn_all(
			&conf.ai,
			state.clone(),
			ai_tx,
			twitch_eventsub_for_ai,
			twitch_moderator_for_ai,
			twitch_default_broadcaster_login,
		)
		.await?;

		{
			let s = state.read().await;
			let registry = s.managed_apps.clone();
			let event_tx = s.control_event_tx.clone();
			let broker_for_monitor = shutdown.clone();
			tokio::spawn(async move {
				managed_app::run_monitor(registry, event_tx, broker_for_monitor).await;
			});
		}

		let (flowgraph_bridges_catalog, flowgraph_trigger, channel_datum_tx) = {
			let s = state.read().await;
			let fg = s.flowgraph.read().await;
			let tx = s.channel_datum_tx.clone();
			if let Some(rt) = fg.as_ref() {
				(bridges::collect_all(&rt.node_meta), rt.trigger(), tx)
			} else {
				(bridges::BridgeCatalog::default(), None, tx)
			}
		};

		let v2_eventsub_skip_broadcasters = {
			let username_fallback = conf.twitch.as_ref().map(|t| t.username.clone()).unwrap_or_default();
			bridges::twitch_eventsub::v1_skip_broadcaster_logins(&flowgraph_bridges_catalog.twitch_eventsub, &username_fallback)
		};

		let (ingress_handles, web_input_registry) =
			processor::ingress::prepare(&conf, state.clone(), &v2_eventsub_skip_broadcasters).await?;

		let initial_bridges = bridges::spawn_all_from_state(&state, &channel_datum_tx).await;
		let flowgraph_web_input_endpoints = std::sync::Arc::new(initial_bridges.web_input_snapshot.clone());
		{
			let s = state.read().await;
			let mut slot = s.bridge_handles.lock().await;
			*slot = initial_bridges;
		}

		let motion_handles = motion::MotionHandles::spawn_all(&conf, shutdown.clone());

		let flowgraph_trigger: std::sync::Arc<Option<flowgraph::node::TriggerHandle>> = std::sync::Arc::new(flowgraph_trigger.clone());

		let control_api_runtime = web_interface::control::ControlApiRuntime::init(&conf, &state).await?;
		log::info!(
			"《ControlAPI》 認証ポリシー: loopback={}, non_loopback={} (token_source={:?})",
			if control_api_runtime.require_token_for_loopback {
				"token-required"
			} else {
				"allow"
			},
			if control_api_runtime.require_token_for_non_loopback {
				"token-required"
			} else {
				"allow"
			},
			control_api_runtime.token_source
		);
		if let Some(path) = control_api_runtime.written_token_file.as_ref() {
			log::info!(
				"《ControlAPI》 自動生成トークンを書き出しました: {:?}（GUI クライアントはこのファイルを読み取って Authorization: Bearer <token> に使う）",
				path
			);
		}

		Ok(Self {
			conf,
			state,
			shutdown,
			ai_handles,
			ingress_handles,
			motion_handles,
			web_input_registry,
			control_api_runtime,
			flowgraph_web_input_endpoints,
			flowgraph_trigger,
		})
	}

	pub async fn serve(&self) -> Result<()> {
		run_services(
			self.conf.clone(),
			self.state.clone(),
			self.web_input_registry.clone(),
			self.control_api_runtime.clone(),
			self.flowgraph_web_input_endpoints.clone(),
			self.flowgraph_trigger.clone(),
			self.shutdown.clone(),
		)
		.await
	}

	pub fn shutdown_broker(&self) -> Arc<shutdown::ShutdownBroker> {
		self.shutdown.clone()
	}

	pub fn gui_url(&self) -> String {
		let address = normalize_loopback_address(self.conf.get_web_ui_address());
		format!("http://{address}/gui/")
	}

	pub async fn cleanup(self) -> Result<()> {
		let managed_stop = {
			let s = self.state.read().await;
			let registry = s.managed_apps.clone();
			log::info!("《Shutdown》 ManagedApp 全停止を試行します（entry ごとの shutdown cfg を使用）。");
			managed_app::stop_all_graceful(&registry).await
		};
		for (id, outcome) in &managed_stop {
			log::info!(
				"《Shutdown》 ManagedApp 停止結果 id={} closed_windows={} terminated_pids={}",
				id,
				outcome.closed_windows,
				outcome.terminated_pids
			);
		}

		self.motion_handles.finish_all().await;

		{
			let handles_arc = self.state.read().await.bridge_handles.clone();
			let mut slot = handles_arc.lock().await;
			let taken = std::mem::replace(&mut *slot, bridges::BridgeHandles::empty());
			taken.finish_all().await;
		}
		for h in self.ingress_handles.eventsub {
			h.abort();
		}
		for h in self.ai_handles {
			h.abort();
		}

		{
			let s = self.state.read().await;
			let fut = async {
				s.libretranslate.lock().await.stop().await;
			};
			if tokio::time::timeout(std::time::Duration::from_secs(5), fut).await.is_err() {
				log::warn!("《Shutdown》 LibreTranslate.stop() が 5 秒以内に完了しませんでした。続行します。");
			}
		}

		log::info!("《Shutdown》 cleanup 完了。プロセスを終了します。");
		Ok(())
	}
}

fn normalize_loopback_address(address: &str) -> String {
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

async fn run_services(
	conf: Conf,
	state: SharedState,
	web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
	control_api_runtime: web_interface::control::ControlApiRuntime,
	flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
	flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
	shutdown: Arc<shutdown::ShutdownBroker>,
) -> Result<()> {
	let workers = conf.get_workers();
	let web_ui_address = conf.get_web_ui_address().to_string();
	let output_root = conf
		.browser_source
		.as_ref()
		.and_then(|b| b.document_root.clone())
		.unwrap_or_else(|| std::path::PathBuf::from("output"));
	let server = actix_web::HttpServer::new(move || {
		let state = state.clone();
		let conf = conf.clone();
		let reg = web_input_registry.clone();
		let output_root = output_root.clone();
		let control_api_runtime = control_api_runtime.clone();
		let fg_endpoints = flowgraph_web_input_endpoints.clone();
		let fg_trigger = flowgraph_trigger.clone();
		let app = actix_web::App::new()
			.wrap(actix_web::middleware::Condition::new(
				conf.web_ui_compress,
				actix_web::middleware::Compress::default(),
			))
			.app_data(Data::new(state))
			.app_data(Data::new(web_interface::output::OutputPaths { root: output_root.clone() }))
			.app_data(Data::new(reg.clone()))
			.app_data(Data::new(control_api_runtime))
			.app_data(Data::new(fg_trigger.clone()))
			.configure({
				let r = reg.clone();
				move |cfg| {
					web_interface::web_input::register_web_input_routes(cfg, &r);
				}
			})
			.configure({
				let eps = fg_endpoints.clone();
				move |cfg| {
					bridges::web_input::register_routes(cfg, eps.as_ref());
				}
			})
			.configure(web_interface::control::register)
			.configure({
				let gui_dist_path = conf.gui_dist_path.clone();
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
		if let Some(web_ui_resources_path) = conf.web_ui_resources_path {
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
