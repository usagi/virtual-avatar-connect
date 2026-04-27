//! Twitch Device Code Flow の開始・ポーリング完了までのコア処理。

use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context, Result};
use tokio::time::Instant as TokioInstant;

use crate::web_interface::control::events::ControlEvent;
use crate::twitch::oauth::{
	self, launch_browser, poll_device_token, save_stored_tokens, start_device_authorization, OAuthIdent, StoredTokens,
};
use crate::twitch_oauth_sessions::{OAuthAccount, OAuthSessionInternal, OAuthSessionStatus};
use crate::SharedState;

use super::OAuthStartResponse;

pub(super) async fn start_oauth(state: &SharedState, account: OAuthAccount) -> Result<OAuthStartResponse> {
	// 1) Build OAuthIdent from config.
	let (sessions, tx, ident) = {
		let s = state.read().await;
		let twitch = s
			.twitch
			.as_ref()
			.cloned()
			.ok_or_else(|| anyhow!("[twitch] section is not configured; add a `[twitch]` section to conf"))?;
		let es = twitch
			.eventsub
			.as_ref()
			.ok_or_else(|| anyhow!("[twitch.eventsub] section is not configured"))?;
		let ident = match account {
			OAuthAccount::Broadcaster => OAuthIdent::for_broadcaster(es),
			OAuthAccount::Moderator => {
				let mc = twitch
					.moderator
					.as_ref()
					.ok_or_else(|| anyhow!("[twitch.moderator] section is not configured; add it to use the moderator account"))?;
				OAuthIdent::for_moderator(es, mc)
			}
		};
		(s.twitch_oauth.clone(), s.control_event_tx.clone(), ident)
	};

	if ident.client_id.is_empty() {
		return Err(anyhow!(
			"Twitch Client ID is empty. Set the env var VAC_TWITCH_CLIENT_ID or `[twitch.eventsub].client_id`."
		));
	}

	// 2) If a Pending session already exists, return it idempotently;
	//    if already Authorized with a valid token, short-circuit.
	{
		if let Some(existing) = sessions.active_pending_snapshot(account, TokioInstant::now()).await {
			log::info!(
				"[ControlAPI/oauth] {}: returning existing Pending session (user_code={})",
				account.as_tag(),
				existing.user_code
			);
			return Ok(OAuthStartResponse {
				outcome: "already_pending",
				session: Some(existing),
			});
		}
	}
	// Stored tokens are still valid -> skip DCF.
	if oauth::try_load_valid_token_for(&ident).await.is_some() {
		log::info!(
			"[ControlAPI/oauth] {}: stored token is valid, DCF will not be started",
			account.as_tag()
		);
		return Ok(OAuthStartResponse {
			outcome: "already_authorized",
			session: None,
		});
	}

	// 3) Start DCF.
	let device = start_device_authorization(&ident.client_id, &ident.scopes)
		.await
		.context("failed to start device_authorization")?;

	// 4) Launch the browser (failure is non-fatal; GUI can open the URI manually).
	if !launch_browser(&ident, &device.verification_uri) {
		log::warn!(
			"[ControlAPI/oauth] {}: failed to open browser. Open {} manually and enter code `{}`",
			account.as_tag(),
			device.verification_uri,
			device.user_code
		);
	}

	// 5) Register session + spawn polling task.
	let timeout = Duration::from_secs(ident.timeout_secs.max(60));
	let started_instant = TokioInstant::now();
	let deadline_instant = started_instant + timeout;
	let started_system = SystemTime::now();
	let expires_system = started_system + timeout;
	let interval_secs = device.interval;

	let sessions_for_task = sessions.clone();
	let tx_for_task = tx.clone();
	let ident_for_task = ident.clone();
	let device_code = device.device_code.clone();
	let user_code = device.user_code.clone();
	let verification_uri = device.verification_uri.clone();

	let handle = tokio::spawn(async move {
		let result = poll_device_token(
			&ident_for_task.client_id,
			&ident_for_task.scopes,
			&device_code,
			interval_secs,
			deadline_instant,
		)
		.await;
		let (new_status, last_error) = match result {
			Ok(tr) => {
				let stored = StoredTokens {
					access_token: tr.access_token.clone(),
					refresh_token: tr.refresh_token,
					scope: tr.scope,
					client_id: Some(ident_for_task.client_id.clone()),
				};
				match save_stored_tokens(&ident_for_task, &stored) {
					Ok(()) => {
						log::info!(
							"[ControlAPI/oauth] {}: DCF complete, token saved (token value not logged)",
							ident_for_task.label
						);
						(OAuthSessionStatus::Authorized, None)
					}
					Err(e) => {
						log::error!(
							"[ControlAPI/oauth] {}: DCF succeeded but saving token failed: {:#}",
							ident_for_task.label,
							e
						);
						(OAuthSessionStatus::Failed, Some(format!("token save failed: {}", e)))
					}
				}
			}
			Err(e) => {
				let msg = format!("{}", e);
				let status = if msg.contains("timeout") || msg.contains("expired") {
					OAuthSessionStatus::Expired
				} else {
					OAuthSessionStatus::Failed
				};
				log::warn!(
					"[ControlAPI/oauth] {}: DCF polling finished: {:?} - {}",
					ident_for_task.label,
					status,
					msg
				);
				(status, Some(msg))
			}
		};
		// Update session map and notify WS subscribers.
		let view = sessions_for_task
			.finish_session(ident_account_of_label(&ident_for_task), new_status, last_error.clone())
			.await;
		if let Some(view) = view {
			let _ = tx_for_task.send(ControlEvent::OAuthStatus {
				account: view.account,
				status: new_status,
				view,
			});
		}
	});

	// 6) Register new session (aborting any stale entry).
	let view = sessions
		.replace_session(
			account,
			OAuthSessionInternal {
				account,
				user_code: user_code.clone(),
				verification_uri: verification_uri.clone(),
				interval_secs,
				expires_at_system: expires_system,
				started_at_system: started_system,
				expires_at_instant: deadline_instant,
				status: OAuthSessionStatus::Pending,
				last_error: None,
				cancel_handle: Some(handle),
			},
		)
		.await;

	log::info!(
		"[ControlAPI/oauth] {}: DCF started (user_code={} uri={})",
		account.as_tag(),
		view.user_code,
		view.verification_uri
	);
	let _ = tx.send(ControlEvent::OAuthStatus {
		account: account.as_tag(),
		status: OAuthSessionStatus::Pending,
		view: view.clone(),
	});

	Ok(OAuthStartResponse {
		outcome: "started",
		session: Some(view),
	})
}

fn ident_account_of_label(ident: &OAuthIdent) -> OAuthAccount {
	match ident.label.as_str() {
		"moderator" => OAuthAccount::Moderator,
		_ => OAuthAccount::Broadcaster,
	}
}
