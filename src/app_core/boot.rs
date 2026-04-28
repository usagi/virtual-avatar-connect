use super::types::{AppCoreServices, AppCoreTasks};
use super::AppCoreParts;
use crate::conf::Conf;
use crate::{motion, shutdown, Result, SharedAudioSink};
use control_api::init_control_api_runtime;
use flowgraph_io::prepare_flowgraph_io;
use services::{spawn_ai_services, spawn_managed_app_monitor};

mod control_api;
mod flowgraph_io;
mod services;

pub(super) async fn boot(conf: Conf, audio_sink: SharedAudioSink) -> Result<AppCoreParts> {
	let shutdown = shutdown::ShutdownBroker::new();
	shutdown::spawn_ctrl_c_listener(shutdown.clone());

	let state = crate::State::new(&conf, audio_sink, shutdown.clone()).await?;

	let ai_handles = spawn_ai_services(&conf, &state).await?;

	spawn_managed_app_monitor(&state, shutdown.clone()).await;

	let flowgraph_io = prepare_flowgraph_io(&conf, &state).await?;

	let motion_handles = motion::MotionHandles::spawn_all(&conf, shutdown.clone());

	let control_api_runtime = init_control_api_runtime(&conf, &state).await?;

	Ok(AppCoreParts {
		conf,
		state,
		shutdown,
		tasks: AppCoreTasks {
			ai_handles,
			ingress_handles: flowgraph_io.ingress_handles,
			motion_handles,
		},
		services: AppCoreServices {
			web_input_registry: flowgraph_io.web_input_registry,
			control_api_runtime,
			flowgraph_web_input_endpoints: flowgraph_io.web_input_endpoints,
			flowgraph_trigger: flowgraph_io.trigger,
		},
	})
}
