//! Phase VI-alpha-5: Control API endpoint group that triggers the Twitch
//! Device Code Flow (DCF) from the GUI.
//!
//! Endpoints (Bearer auth is handled by outer `control_api_auth`):
//!   - `POST   /api/v1/control/oauth/twitch/{account}/start`   - begin DCF (idempotent if pending)
//!   - `GET    /api/v1/control/oauth/twitch/{account}/status`  - current session status
//!   - `POST   /api/v1/control/oauth/twitch/{account}/cancel`  - cancel in-flight session
//!   - `DELETE /api/v1/control/oauth/twitch/{account}/tokens`  - purge stored tokens (force re-auth)
//!
//! `account` must be `broadcaster` or `moderator`. If the corresponding config
//! (`[twitch.eventsub]` / `[twitch.moderator]`) is missing we return 400.
//!
//! The WS `/events` stream emits `ControlEvent::OAuthStatus` so GUIs can
//! follow progress without polling `status`.

use actix_web::web::{self, Data};
use actix_web::{delete, get, post, HttpResponse, Responder};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;

use super::events::ControlEvent;
use crate::twitch::oauth::{
	self, launch_browser, poll_device_token, save_stored_tokens, start_device_authorization, OAuthIdent, StoredTokens,
};
use crate::SharedState;

/// Logical Twitch OAuth account handled by this endpoint group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthAccount {
	Broadcaster,
	Moderator,
}

impl OAuthAccount {
	pub fn as_tag(self) -> &'static str {
		match self {
			Self::Broadcaster => "broadcaster",
			Self::Moderator => "moderator",
		}
	}

	pub fn from_path(s: &str) -> Option<Self> {
		match s {
			"broadcaster" => Some(Self::Broadcaster),
			"moderator" => Some(Self::Moderator),
			_ => None,
		}
	}
}

/// Lifecycle state of a DCF session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthSessionStatus {
	/// DCF started, awaiting user confirmation in the browser.
	Pending,
	/// Authorization complete, tokens stored.
	Authorized,
	/// Timed out before user confirmation.
	Expired,
	/// User invoked `cancel`.
	Canceled,
	/// HTTP/JSON or other unexpected failure; see `last_error`.
	Failed,
}

/// GUI-facing session snapshot (no secrets such as `device_code`).
#[derive(Debug, Clone, Serialize)]
pub struct OAuthSessionView {
	pub account: &'static str,
	pub status: OAuthSessionStatus,
	pub user_code: String,
	pub verification_uri: String,
	pub interval_secs: u64,
	/// `started_at + timeout` in UTC RFC3339.
	pub expires_at: String,
	/// Session start time (UTC RFC3339).
	pub started_at: String,
	/// Populated on `Failed` / `Expired`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub last_error: Option<String>,
}

/// Internal session. `device_code` and `cancel_handle` are not exposed to GUIs.
#[derive(Debug)]
struct OAuthSessionInternal {
	account: OAuthAccount,
	user_code: String,
	verification_uri: String,
	interval_secs: u64,
	expires_at_system: SystemTime,
	started_at_system: SystemTime,
	expires_at_instant: TokioInstant,
	status: OAuthSessionStatus,
	last_error: Option<String>,
	/// Handle to the polling task so `cancel` can abort it.
	cancel_handle: Option<JoinHandle<()>>,
}

impl OAuthSessionInternal {
	fn to_view(&self) -> OAuthSessionView {
		OAuthSessionView {
			account: self.account.as_tag(),
			status: self.status,
			user_code: self.user_code.clone(),
			verification_uri: self.verification_uri.clone(),
			interval_secs: self.interval_secs,
			expires_at: system_time_to_rfc3339(self.expires_at_system),
			started_at: system_time_to_rfc3339(self.started_at_system),
			last_error: self.last_error.clone(),
		}
	}
}

fn system_time_to_rfc3339(t: SystemTime) -> String {
	jiff::Timestamp::try_from(t).map(|ts| ts.to_string()).unwrap_or_default()
}

/// Session map. `State` holds an `Arc` of this.
#[derive(Debug)]
pub struct OAuthSessions {
	inner: RwLock<std::collections::HashMap<OAuthAccount, OAuthSessionInternal>>,
}

impl OAuthSessions {
	pub fn new() -> Arc<Self> {
		Arc::new(Self {
			inner: RwLock::new(std::collections::HashMap::new()),
		})
	}

	pub async fn snapshot(&self, account: OAuthAccount) -> Option<OAuthSessionView> {
		self.inner.read().await.get(&account).map(OAuthSessionInternal::to_view)
	}

	pub async fn snapshot_all(&self) -> Vec<OAuthSessionView> {
		self.inner.read().await.values().map(OAuthSessionInternal::to_view).collect()
	}
}

impl Default for OAuthSessions {
	fn default() -> Self {
		Self {
			inner: RwLock::new(std::collections::HashMap::new()),
		}
	}
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct OAuthStartResponse {
	/// One of: "started" (new), "already_pending", "already_authorized".
	pub outcome: &'static str,
	pub session: Option<OAuthSessionView>,
}

#[post("/oauth/twitch/{account}/start")]
pub async fn post_oauth_start(state: Data<SharedState>, path: web::Path<String>) -> impl Responder {
	let Some(account) = OAuthAccount::from_path(&path.into_inner()) else {
		return bad_request("account must be `broadcaster` or `moderator`");
	};

	match start_oauth(state.get_ref(), account).await {
		Ok(resp) => HttpResponse::Ok().json(resp),
		Err(e) => {
			log::error!("[ControlAPI/oauth] start {} failed: {:#}", account.as_tag(), e);
			HttpResponse::BadRequest().json(serde_json::json!({
			 "error": "oauth_start_failed",
			 "reason": e.to_string(),
			}))
		}
	}
}

#[get("/oauth/twitch/{account}/status")]
pub async fn get_oauth_status(state: Data<SharedState>, path: web::Path<String>) -> impl Responder {
	let Some(account) = OAuthAccount::from_path(&path.into_inner()) else {
		return bad_request("account must be `broadcaster` or `moderator`");
	};

	let sessions = {
		let s = state.read().await;
		s.twitch_oauth.clone()
	};
	match sessions.snapshot(account).await {
		Some(view) => HttpResponse::Ok().json(view),
		None => HttpResponse::NotFound().json(serde_json::json!({
		 "error": "no_session",
		 "account": account.as_tag(),
		})),
	}
}

#[post("/oauth/twitch/{account}/cancel")]
pub async fn post_oauth_cancel(state: Data<SharedState>, path: web::Path<String>) -> impl Responder {
	let Some(account) = OAuthAccount::from_path(&path.into_inner()) else {
		return bad_request("account must be `broadcaster` or `moderator`");
	};

	let (sessions, tx) = {
		let s = state.read().await;
		(s.twitch_oauth.clone(), s.control_event_tx.clone())
	};

	let canceled = {
		let mut map = sessions.inner.write().await;
		let Some(session) = map.get_mut(&account) else {
			return HttpResponse::NotFound().json(serde_json::json!({
			 "error": "no_session",
			 "account": account.as_tag(),
			}));
		};
		match session.status {
			OAuthSessionStatus::Pending => {
				if let Some(h) = session.cancel_handle.take() {
					h.abort();
				}
				session.status = OAuthSessionStatus::Canceled;
				true
			}
			_ => false,
		}
	};

	let view = sessions.snapshot(account).await.unwrap();
	if canceled {
		log::info!("[ControlAPI/oauth] {} session canceled", account.as_tag());
		let _ = tx.send(ControlEvent::OAuthStatus {
			account: account.as_tag(),
			status: OAuthSessionStatus::Canceled,
			view: view.clone(),
		});
	}
	HttpResponse::Ok().json(serde_json::json!({
	 "canceled": canceled,
	 "session": view,
	}))
}

/// Purges stored tokens for the given account.
///
/// Use case: the user notices "I authorized the wrong account" or a stale
/// token is misbehaving, and wants a forced reset.
///
/// Behaviour:
///   - Removes the token file (no-op if absent).
///   - Removes any in-flight / completed OAuth session from the map
///     (aborts the polling task if it is still Pending).
///   - A subsequent `GET /snapshot` will report `twitch.{account}_authorized`
///     as `false`.
///
/// Note: currently running EventSub WebSocket connections keep their
/// in-memory access tokens; restart VAC for a clean reset.
#[delete("/oauth/twitch/{account}/tokens")]
pub async fn delete_oauth_tokens(state: Data<SharedState>, path: web::Path<String>) -> impl Responder {
	let Some(account) = OAuthAccount::from_path(&path.into_inner()) else {
		return bad_request("account must be `broadcaster` or `moderator`");
	};

	// Build ident first (delete is a no-op if config is missing).
	let ident = {
		let s = state.read().await;
		let Some(twitch) = s.twitch.as_ref().cloned() else {
			return HttpResponse::BadRequest().json(serde_json::json!({
			 "error": "no_twitch_config",
			 "reason": "[twitch] section is not configured",
			}));
		};
		let Some(es) = twitch.eventsub.as_ref() else {
			return HttpResponse::BadRequest().json(serde_json::json!({
			 "error": "no_eventsub_config",
			 "reason": "[twitch.eventsub] section is not configured",
			}));
		};
		match account {
			OAuthAccount::Broadcaster => OAuthIdent::for_broadcaster(es),
			OAuthAccount::Moderator => {
				let Some(mc) = twitch.moderator.as_ref() else {
					return HttpResponse::BadRequest().json(serde_json::json!({
					 "error": "no_moderator_config",
					 "reason": "[twitch.moderator] section is not configured",
					}));
				};
				OAuthIdent::for_moderator(es, mc)
			}
		}
	};

	let deleted_path = match oauth::delete_stored_tokens(&ident) {
		Ok(opt) => opt,
		Err(e) => {
			log::error!("[ControlAPI/oauth] tokens delete {} failed: {:#}", account.as_tag(), e);
			return HttpResponse::InternalServerError().json(serde_json::json!({
			 "error": "delete_failed",
			 "reason": e.to_string(),
			}));
		}
	};

	// Remove any existing session (aborting Pending polls).
	let session_removed = {
		let sessions = {
			let s = state.read().await;
			s.twitch_oauth.clone()
		};
		let mut map = sessions.inner.write().await;
		if let Some(s) = map.remove(&account) {
			if let Some(h) = s.cancel_handle {
				h.abort();
			}
			true
		} else {
			false
		}
	};

	log::info!(
		"[ControlAPI/oauth] {}: tokens delete done (deleted_file={} session_removed={})",
		account.as_tag(),
		deleted_path.is_some(),
		session_removed,
	);

	HttpResponse::Ok().json(serde_json::json!({
	 "account": account.as_tag(),
	 "deleted": deleted_path.is_some(),
	 "path": deleted_path.map(|p| p.display().to_string()),
	 "session_removed": session_removed,
	}))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(post_oauth_start);
	cfg.service(get_oauth_status);
	cfg.service(post_oauth_cancel);
	cfg.service(delete_oauth_tokens);
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

async fn start_oauth(state: &SharedState, account: OAuthAccount) -> Result<OAuthStartResponse> {
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
		let map = sessions.inner.read().await;
		if let Some(existing) = map.get(&account) {
			if matches!(existing.status, OAuthSessionStatus::Pending) && TokioInstant::now() < existing.expires_at_instant {
				log::info!(
					"[ControlAPI/oauth] {}: returning existing Pending session (user_code={})",
					account.as_tag(),
					existing.user_code
				);
				return Ok(OAuthStartResponse {
					outcome: "already_pending",
					session: Some(existing.to_view()),
				});
			}
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
		let view = {
			let mut map = sessions_for_task.inner.write().await;
			if let Some(s) = map.get_mut(&ident_account_of_label(&ident_for_task)) {
				s.status = new_status;
				s.last_error = last_error.clone();
				s.cancel_handle = None;
				Some(s.to_view())
			} else {
				None
			}
		};
		if let Some(view) = view {
			let _ = tx_for_task.send(ControlEvent::OAuthStatus {
				account: view.account,
				status: new_status,
				view,
			});
		}
	});

	// 6) Register new session (aborting any stale entry).
	let view = {
		let mut map = sessions.inner.write().await;
		if let Some(old) = map.insert(
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
		) {
			if let Some(h) = old.cancel_handle {
				h.abort();
			}
		}
		map.get(&account).unwrap().to_view()
	};

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

fn bad_request(reason: &str) -> HttpResponse {
	HttpResponse::BadRequest()
		.content_type("application/json")
		.body(format!(r#"{{"error":"bad_request","reason":"{}"}}"#, reason.replace('"', "\\\"")))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn account_from_path() {
		assert_eq!(OAuthAccount::from_path("broadcaster"), Some(OAuthAccount::Broadcaster));
		assert_eq!(OAuthAccount::from_path("moderator"), Some(OAuthAccount::Moderator));
		assert_eq!(OAuthAccount::from_path("viewer"), None);
		assert_eq!(OAuthAccount::from_path(""), None);
	}

	#[test]
	fn account_tag_roundtrip() {
		for a in [OAuthAccount::Broadcaster, OAuthAccount::Moderator] {
			let t = a.as_tag();
			assert_eq!(OAuthAccount::from_path(t), Some(a));
		}
	}

	#[test]
	fn session_view_serializes() {
		let internal = OAuthSessionInternal {
			account: OAuthAccount::Broadcaster,
			user_code: "AB12CD".to_string(),
			verification_uri: "https://www.twitch.tv/activate?public=true&device-code=AB12CD".to_string(),
			interval_secs: 5,
			expires_at_system: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
			started_at_system: SystemTime::UNIX_EPOCH + Duration::from_secs(1_699_999_400),
			expires_at_instant: TokioInstant::now(),
			status: OAuthSessionStatus::Pending,
			last_error: None,
			cancel_handle: None,
		};
		let view = internal.to_view();
		let json = serde_json::to_string(&view).unwrap();
		assert!(json.contains("\"account\":\"broadcaster\""));
		assert!(json.contains("\"status\":\"pending\""));
		assert!(json.contains("\"user_code\":\"AB12CD\""));
	}
}
