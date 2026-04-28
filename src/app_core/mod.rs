//! Step 7（再構造化）: 通常運転の **bootstrap + HTTP serve + shutdown cleanup**。
//!
//! `crate::run()` はロガー・CLI 特殊モード・conf ロードまでを担当し、本モジュールが
//! `ShutdownBroker` 以降の常駐ランタイム本体をまとめる。将来 `vac-app` crate へ移す際の境界の目印。

mod boot;
mod cleanup;
mod server;

use crate::bridges;
use crate::conf::Conf;
use crate::motion;
use crate::processor;
use crate::shutdown;
use crate::state::SharedState;
use crate::{flowgraph, web_interface, Result, SharedAudioSink};
use std::sync::Arc;

/// conf ロード済み・`run_with` 済みの状態から起動する VAC 常駐ランタイム本体。
///
/// CLI / desktop runner は、最終的にこの `boot` / `serve` / `cleanup`
/// 境界を共有する。現時点では `run_vac_application` が従来通り直列に呼ぶ。
pub(crate) struct AppCore {
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
	pub(crate) async fn boot(conf: Conf, audio_sink: SharedAudioSink) -> Result<Self> {
		boot::boot(conf, audio_sink).await
	}

	pub(crate) async fn serve(&self) -> Result<()> {
		server::run_services(
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

	pub(crate) fn shutdown_broker(&self) -> Arc<shutdown::ShutdownBroker> {
		self.shutdown.clone()
	}

	pub(crate) fn gui_url(&self) -> String {
		let address = server::normalize_loopback_address(self.conf.get_web_ui_address());
		format!("http://{address}/gui/")
	}

	pub(crate) async fn cleanup(self) -> Result<()> {
		cleanup::cleanup(self).await
	}
}
