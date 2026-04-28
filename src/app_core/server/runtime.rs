use super::AppCoreParts;
use crate::conf::Conf;
use crate::state::SharedState;
use crate::{bridges, flowgraph, shutdown, web_interface};
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_BROWSER_OUTPUT_ROOT: &str = "output";

#[derive(Clone)]
pub(super) struct ServerRuntime {
	pub(super) conf: Conf,
	pub(super) state: SharedState,
	pub(super) web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
	pub(super) control_api_runtime: web_interface::control::ControlApiRuntime,
	pub(super) flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
	pub(super) flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
	pub(super) shutdown: Arc<shutdown::ShutdownBroker>,
	pub(super) output_root: PathBuf,
	pub(super) workers: usize,
	pub(super) web_ui_address: String,
}

impl ServerRuntime {
	pub(super) fn from_app_core(parts: &AppCoreParts) -> Self {
		let conf = parts.conf.clone();
		let output_root = conf
			.browser_source
			.as_ref()
			.and_then(|b| b.document_root.clone())
			.unwrap_or_else(|| PathBuf::from(DEFAULT_BROWSER_OUTPUT_ROOT));
		let workers = conf.get_workers();
		let web_ui_address = conf.get_web_ui_address().to_string();
		Self {
			conf,
			state: parts.state.clone(),
			web_input_registry: parts.services.web_input_registry.clone(),
			control_api_runtime: parts.services.control_api_runtime.clone(),
			flowgraph_web_input_endpoints: parts.services.flowgraph_web_input_endpoints.clone(),
			flowgraph_trigger: parts.services.flowgraph_trigger.clone(),
			shutdown: parts.shutdown.clone(),
			output_root,
			workers,
			web_ui_address,
		}
	}
}
