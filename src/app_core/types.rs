use super::address;
use crate::conf::Conf;
use crate::state::SharedState;
use crate::{bridges, flowgraph, motion, processor, shutdown, web_interface, Result};
use std::sync::Arc;

pub(crate) struct AppCoreParts {
	pub(super) conf: Conf,
	pub(super) state: SharedState,
	pub(super) shutdown: Arc<shutdown::ShutdownBroker>,
	pub(super) ai_handles: Vec<tokio::task::JoinHandle<()>>,
	pub(super) ingress_handles: processor::ingress::IngressHandles,
	pub(super) motion_handles: motion::MotionHandles,
	pub(super) web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
	pub(super) control_api_runtime: web_interface::control::ControlApiRuntime,
	pub(super) flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
	pub(super) flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
}

pub(crate) struct AppCoreRuntimeHandle {
	pub(crate) gui_url: String,
	pub(crate) shutdown: Arc<shutdown::ShutdownBroker>,
}

pub(crate) struct AppCoreRunResult {
	pub(crate) serve: Result<()>,
	pub(crate) cleanup: Result<()>,
}

impl AppCoreRunResult {
	pub(crate) fn into_result(self) -> Result<()> {
		self.serve?;
		self.cleanup
	}
}

impl AppCoreParts {
	pub(super) fn runtime_handle(&self) -> AppCoreRuntimeHandle {
		let address = address::normalize_loopback_address(self.conf.get_web_ui_address());
		AppCoreRuntimeHandle {
			gui_url: format!("http://{address}/gui/"),
			shutdown: self.shutdown.clone(),
		}
	}
}
