use crate::conf::Conf;
use crate::state::SharedState;
use crate::{bridges, flowgraph, motion, processor, shutdown, web_interface};
use std::sync::Arc;

pub(in crate::app_core) struct AppCoreParts {
	pub(in crate::app_core) conf: Conf,
	pub(in crate::app_core) state: SharedState,
	pub(in crate::app_core) shutdown: Arc<shutdown::ShutdownBroker>,
	pub(in crate::app_core) tasks: AppCoreTasks,
	pub(in crate::app_core) services: AppCoreServices,
}

pub(in crate::app_core) struct AppCoreTasks {
	pub(in crate::app_core) ai_handles: Vec<tokio::task::JoinHandle<()>>,
	pub(in crate::app_core) ingress_handles: processor::ingress::IngressHandles,
	pub(in crate::app_core) motion_handles: motion::MotionHandles,
}

pub(in crate::app_core) struct AppCoreServices {
	pub(in crate::app_core) web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
	pub(in crate::app_core) control_api_runtime: web_interface::control::ControlApiRuntime,
	pub(in crate::app_core) flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
	pub(in crate::app_core) flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
}

impl AppCoreTasks {
	pub(in crate::app_core) fn new(
		ai_handles: Vec<tokio::task::JoinHandle<()>>,
		ingress_handles: processor::ingress::IngressHandles,
		motion_handles: motion::MotionHandles,
	) -> Self {
		Self {
			ai_handles,
			ingress_handles,
			motion_handles,
		}
	}
}

impl AppCoreServices {
	pub(in crate::app_core) fn new(
		web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
		control_api_runtime: web_interface::control::ControlApiRuntime,
		flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
		flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
	) -> Self {
		Self {
			web_input_registry,
			control_api_runtime,
			flowgraph_web_input_endpoints,
			flowgraph_trigger,
		}
	}
}

impl AppCoreParts {
	pub(in crate::app_core) fn new(
		conf: Conf,
		state: SharedState,
		shutdown: Arc<shutdown::ShutdownBroker>,
		tasks: AppCoreTasks,
		services: AppCoreServices,
	) -> Self {
		Self {
			conf,
			state,
			shutdown,
			tasks,
			services,
		}
	}
}
