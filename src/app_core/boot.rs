use super::*;

pub(super) async fn boot(conf: Conf, audio_sink: SharedAudioSink) -> Result<AppCore> {
	let shutdown = shutdown::ShutdownBroker::new();
	shutdown::spawn_ctrl_c_listener(shutdown.clone());

	let state = crate::State::new(&conf, audio_sink, shutdown.clone()).await?;

	let ai_tx = state.read().await.ai_observation_tx.clone();
	let twitch_eventsub_for_ai = conf
		.twitch
		.as_ref()
		.and_then(|t| t.eventsub.as_ref())
		.map(|e| Arc::new(e.clone()));
	let twitch_moderator_for_ai = conf
		.twitch
		.as_ref()
		.and_then(|t| t.moderator.as_ref())
		.map(|m| Arc::new(m.clone()));
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
	let flowgraph_web_input_endpoints = Arc::new(initial_bridges.web_input_snapshot.clone());
	{
		let s = state.read().await;
		let mut slot = s.bridge_handles.lock().await;
		*slot = initial_bridges;
	}

	let motion_handles = motion::MotionHandles::spawn_all(&conf, shutdown.clone());

	let flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>> = Arc::new(flowgraph_trigger.clone());

	let control_api_runtime = web_interface::control::ControlApiRuntime::init(&conf, &state).await?;
	log_control_api_policy(&control_api_runtime);

	Ok(AppCore {
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
