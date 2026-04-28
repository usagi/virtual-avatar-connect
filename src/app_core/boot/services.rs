use crate::conf::Conf;
use crate::state::SharedState;
use crate::{ai, managed_app, shutdown, Result};
use std::sync::Arc;

pub(super) async fn spawn_ai_services(conf: &Conf, state: &SharedState) -> Result<Vec<tokio::task::JoinHandle<()>>> {
	let ai_tx = state.read().await.ai_observation_tx.clone();
	let twitch_eventsub_for_ai = conf.twitch.as_ref().and_then(|t| t.eventsub.as_ref()).map(|e| Arc::new(e.clone()));
	let twitch_moderator_for_ai = conf.twitch.as_ref().and_then(|t| t.moderator.as_ref()).map(|m| Arc::new(m.clone()));
	let twitch_default_broadcaster_login = conf.twitch.as_ref().map(|t| {
		t.eventsub
			.as_ref()
			.and_then(|e| e.broadcaster_login.clone())
			.unwrap_or_else(|| t.username.clone())
	});
	Ok(ai::spawn_all(
		&conf.ai,
		state.clone(),
		ai_tx,
		twitch_eventsub_for_ai,
		twitch_moderator_for_ai,
		twitch_default_broadcaster_login,
	)
	.await?)
}

pub(super) async fn spawn_managed_app_monitor(state: &SharedState, shutdown: Arc<shutdown::ShutdownBroker>) {
	let s = state.read().await;
	let registry = s.managed_apps.clone();
	let event_tx = s.control_event_tx.clone();
	tokio::spawn(async move {
		managed_app::run_monitor(registry, event_tx, shutdown).await;
	});
}
