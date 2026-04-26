//! Control API の **`GET /snapshot` 用 read-only DTO**。
//!
//! 外部型（`AiPersonaConf` など）をそのまま serialize するとフィールドのカップリングや将来の互換に弱いので、
//! ここで型を切り分けて GUI 向けの表示・診断用 DTO を明示する。
//!
//! δ-9 (v0.9.x) で V1 processor 層を除去したため、`processors` フィールドはスキーマから外している。
//! GUI のフローレイアウトは `GET /api/v1/control/flowgraph/*` 系で取得する想定。
//!
//! Twitch のトークン有効性判定は [`twitch_summary`]。

mod twitch_summary;

use serde::Serialize;

use crate::state::{AiRuntime, State};

use twitch_summary::compute_twitch_authorized;

/// `/api/v1/control/snapshot` のルート DTO。
#[derive(Debug, Serialize)]
pub struct StateSnapshot {
	/// API DTO のスキーマバージョン。互換性破壊時にインクリメント。
	/// v0.9 で V1 processor 層を除去した際に schema=2 に更新。
	pub schema: u32,
	pub app_version: &'static str,
	pub now: String,
	pub runtime: RuntimeSummary,
	pub ai_personas: Vec<AiPersonaSummary>,
	pub twitch: Option<TwitchSummary>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeSummary {
	pub session_id: String,
	pub root: String,
	pub session_dir: String,
	pub inline_max_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct AiPersonaSummary {
	pub index: usize,
	pub id: Option<String>,
	pub paused: bool,
}

#[derive(Debug, Serialize)]
pub struct TwitchSummary {
	pub username: String,
	pub channel_to: String,
	pub reads: Option<Vec<String>>,
	pub eventsub_enabled: bool,
	pub moderator_enabled: bool,
	pub moderator_login: Option<String>,
	pub ignore_logins: Vec<String>,
	/// broadcaster 用の保存トークンが有効か。`eventsub` 設定が無いなど ident が組めないときは false。
	pub broadcaster_authorized: bool,
	/// moderator 用の保存トークンが有効か。
	pub moderator_authorized: Option<bool>,
}

/// `State` から構築できる read-only な snapshot を組み立てる。
pub async fn snapshot(state: &State) -> StateSnapshot {
	let runtime = RuntimeSummary {
		session_id: state.runtime_paths.session_id.clone(),
		root: state.runtime_paths.root.display().to_string(),
		session_dir: state.runtime_paths.session_dir.display().to_string(),
		inline_max_bytes: state.runtime_paths.inline_max_bytes,
	};

	let ai_personas = {
		let guard = state.ai_runtimes.read().await;
		guard
			.iter()
			.enumerate()
			.map(|(i, rt)| ai_persona_summary(i, rt))
			.collect::<Vec<_>>()
	};

	let twitch = if let Some(t) = state.twitch.as_ref() {
		let ignore_logins = t.ignore_logins.as_ref().cloned().unwrap_or_default();

		let (broadcaster_authorized, moderator_authorized) = compute_twitch_authorized(t).await;

		Some(TwitchSummary {
			username: t.username.clone(),
			channel_to: t.effective_channel_to().to_string(),
			reads: t.reads.clone(),
			eventsub_enabled: t.eventsub.as_ref().map(|e| e.enabled).unwrap_or(false),
			moderator_enabled: t.moderator.as_ref().map(|m| m.enabled).unwrap_or(false),
			moderator_login: t.moderator.as_ref().and_then(|m| m.login.clone()),
			ignore_logins,
			broadcaster_authorized,
			moderator_authorized,
		})
	} else {
		None
	};

	StateSnapshot {
		schema: 2,
		app_version: env!("CARGO_PKG_VERSION"),
		now: jiff::Timestamp::now().to_string(),
		runtime,
		ai_personas,
		twitch,
	}
}

fn ai_persona_summary(index: usize, rt: &AiRuntime) -> AiPersonaSummary {
	AiPersonaSummary {
		index,
		id: rt.persona_id.clone(),
		paused: rt.is_paused(),
	}
}
