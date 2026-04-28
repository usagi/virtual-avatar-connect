use super::AppCoreParts;
use crate::conf::Conf;
use crate::{motion, shutdown, web_interface, Result, SharedAudioSink};
use flowgraph_io::prepare_flowgraph_io;
use services::{spawn_ai_services, spawn_managed_app_monitor};

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

	let control_api_runtime = web_interface::control::ControlApiRuntime::init(&conf, &state).await?;
	log_control_api_policy(&control_api_runtime);

	Ok(AppCoreParts {
		conf,
		state,
		shutdown,
		ai_handles,
		ingress_handles: flowgraph_io.ingress_handles,
		motion_handles,
		web_input_registry: flowgraph_io.web_input_registry,
		control_api_runtime,
		flowgraph_web_input_endpoints: flowgraph_io.web_input_endpoints,
		flowgraph_trigger: flowgraph_io.trigger,
	})
}

fn log_control_api_policy(control_api_runtime: &web_interface::control::ControlApiRuntime) {
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
}
