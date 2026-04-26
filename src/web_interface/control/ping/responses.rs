//! `/ping` / `/whoami` の JSON 応答形。

use serde::Serialize;

use crate::web_interface::control::auth::{ControlApiRuntime, TokenSource};

#[derive(Serialize)]
pub(super) struct Pong {
	pub ok: bool,
	pub service: &'static str,
	pub version: &'static str,
	pub now: String,
}

#[derive(Serialize)]
pub(super) struct WhoAmI {
	pub peer_addr: Option<String>,
	pub is_loopback: bool,
	pub required_token: bool,
	pub token_source: &'static str,
	pub token_file: Option<String>,
}

impl WhoAmI {
	pub(super) fn from_runtime(peer: Option<String>, is_loopback: bool, runtime: &ControlApiRuntime) -> Self {
		Self {
			peer_addr: peer,
			is_loopback,
			required_token: runtime.require_token_for(is_loopback),
			token_source: match runtime.token_source {
				TokenSource::Env => "env",
				TokenSource::Config => "config",
				TokenSource::Generated => "generated",
			},
			token_file: runtime.written_token_file.as_ref().map(|p| p.display().to_string()),
		}
	}
}
