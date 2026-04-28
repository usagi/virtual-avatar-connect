use crate::conf::{Conf, Twitch, TwitchEventSubConfig, TwitchModeratorConfig};
use crate::state::SharedState;
use crate::{ai, managed_app, shutdown, Result};
use std::sync::Arc;

struct TwitchAiContext {
	eventsub: Option<Arc<TwitchEventSubConfig>>,
	moderator: Option<Arc<TwitchModeratorConfig>>,
	default_broadcaster_login: Option<String>,
}

pub(super) async fn spawn_ai_services(conf: &Conf, state: &SharedState) -> Result<Vec<tokio::task::JoinHandle<()>>> {
	let ai_tx = state.read().await.ai_observation_tx.clone();
	let twitch = TwitchAiContext::from_conf(conf.twitch.as_ref());
	Ok(ai::spawn_all(
		&conf.ai,
		state.clone(),
		ai_tx,
		twitch.eventsub,
		twitch.moderator,
		twitch.default_broadcaster_login,
	)
	.await?)
}

impl TwitchAiContext {
	fn from_conf(twitch: Option<&Twitch>) -> Self {
		let Some(twitch) = twitch else {
			return Self {
				eventsub: None,
				moderator: None,
				default_broadcaster_login: None,
			};
		};
		let default_broadcaster_login = Some(
			twitch
				.eventsub
				.as_ref()
				.and_then(|e| e.broadcaster_login.clone())
				.unwrap_or_else(|| twitch.username.clone()),
		);
		Self {
			eventsub: twitch.eventsub.as_ref().map(|e| Arc::new(e.clone())),
			moderator: twitch.moderator.as_ref().map(|m| Arc::new(m.clone())),
			default_broadcaster_login,
		}
	}
}

pub(super) async fn spawn_managed_app_monitor(state: &SharedState, shutdown: Arc<shutdown::ShutdownBroker>) {
	let s = state.read().await;
	let registry = s.managed_apps.clone();
	let event_tx = s.control_event_tx.clone();
	tokio::spawn(async move {
		managed_app::run_monitor(registry, event_tx, shutdown).await;
	});
}
