//! Twitch Device Code Flow のセッション表（Control API と [`crate::state::State`] で共有）。
//!
//! HTTP ハンドラは [`crate::web_interface::control::oauth_twitch`] に置く。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;

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
pub struct OAuthSessionInternal {
	pub account: OAuthAccount,
	pub user_code: String,
	pub verification_uri: String,
	pub interval_secs: u64,
	pub expires_at_system: SystemTime,
	pub started_at_system: SystemTime,
	pub expires_at_instant: TokioInstant,
	pub status: OAuthSessionStatus,
	pub last_error: Option<String>,
	/// Handle to the polling task so `cancel` can abort it.
	pub cancel_handle: Option<JoinHandle<()>>,
}

impl OAuthSessionInternal {
	pub fn to_view(&self) -> OAuthSessionView {
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
	pub inner: RwLock<HashMap<OAuthAccount, OAuthSessionInternal>>,
}

impl OAuthSessions {
	pub fn new() -> Arc<Self> {
		Arc::new(Self {
			inner: RwLock::new(HashMap::new()),
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
			inner: RwLock::new(HashMap::new()),
		}
	}
}
