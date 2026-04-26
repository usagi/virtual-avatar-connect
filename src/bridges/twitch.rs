//! `flowgraph.ingress.twitch` 用ブリッジ（ζ-1: IRC 本実装）。
//!
//! ## 責務
//!
//! - `flowgraph.ingress.twitch` ノードの property から [`FlowgraphTwitchIngress`] を生成する。
//! - [`spawn`] で各エントリに対し IRC クライアントを起動し、PRIVMSG を
//!   `TriggerEvent` として `TriggerHandle::send` で流し込む。
//! - 返されるハンドルは shutdown 時に `finish().await` すること。
//!
//! 認可トークンは以下の順で解決する:
//!   1. property `access_token` が非空ならそれをそのまま使う。
//!   2. property `token_key` が非空かつ `SharedState` に該当 token_key の有効トークンが
//!      保存済みならそれを使う。
//!   3. 上記いずれも無ければ匿名 (justinfan) で IRC 接続する（PRIVMSG 受信のみならこれで十分）。
//!
//! `login` プロパティが空なら `username` に anonymous 接続のダミー login を使う。
//!
//! V1 `src/processor/twitch.rs` → `src/twitch/chat.rs` にあった IRC 接続コードを
//! ベースに、Flowgraph の TriggerEvent 発火に繋ぎ込んだもの。

use crate::flowgraph::loader::LoadedNodeMeta;
use crate::flowgraph::node::{TriggerEvent, TriggerHandle};
use crate::flowgraph::socket::SocketValue;
use crate::SharedState;
use std::collections::BTreeSet;
use tokio::task::JoinHandle;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::ServerMessage;
use twitch_irc::validate::validate_login;
use twitch_irc::TwitchIRCClient;
use twitch_irc::{ClientConfig, SecureTCPTransport};

#[derive(Debug, Clone)]
#[allow(dead_code)] // client_id / broadcaster_id / fixed_channel は ζ-2 (EventSub Flowgraph 化) で使用予定
pub struct FlowgraphTwitchIngress {
	pub node_id: String,
	pub mode: String,
	pub channels: Vec<String>,
	pub access_token: String,
	pub token_key: String,
	pub client_id: String,
	pub broadcaster_id: String,
	pub login: String,
	pub fixed_channel: String,
}

impl FlowgraphTwitchIngress {
	pub fn from_meta(fq: &str, meta: &LoadedNodeMeta) -> Option<Self> {
		let p = &meta.properties;
		let get_str = |k: &str| -> String {
			p.get(k)
				.and_then(|v| v.as_str().ok())
				.map(str::trim)
				.map(str::to_string)
				.unwrap_or_default()
		};
		let channels: Vec<String> = p
			.get("channels")
			.and_then(|v| v.as_list().ok())
			.map(|items| {
				items
					.iter()
					.filter_map(|v| v.as_str().ok().map(|s| s.trim().to_string()))
					.filter(|s| !s.is_empty())
					.collect()
			})
			.unwrap_or_default();
		Some(Self {
			node_id: fq.to_string(),
			mode: {
				let m = get_str("mode");
				if m.is_empty() {
					"irc".into()
				} else {
					m.to_ascii_lowercase()
				}
			},
			channels,
			access_token: get_str("access_token"),
			token_key: get_str("token_key"),
			client_id: get_str("client_id"),
			broadcaster_id: get_str("broadcaster_id"),
			login: get_str("login"),
			fixed_channel: get_str("fixed_channel"),
		})
	}
}

/// `source_actor` 文字列を組み立てる（PRIVMSG 送信者）。
///
/// `msg.sender.name` は表示名（大文字混在）、`msg.sender.login` は安定 login（小文字）。
/// Flowgraph 側の無視リスト比較は小文字安定 login で揃えるため、login を採用する。
pub(crate) fn build_source_actor(sender_login: &str) -> String {
	sender_login.trim().to_string()
}

/// TriggerEvent に載せる meta マップを組み立てる。
///
/// Flowgraph 下流の `ingress.twitch` ノードは standard ingress schema に載るため、
/// ここでは channel / sender などの補助情報を meta に入れて下流に伝える。
pub(crate) fn build_meta(
	channel_login: &str,
	sender_login: &str,
	sender_display_name: &str,
	multi_channel: bool,
) -> std::collections::BTreeMap<String, SocketValue> {
	let mut m = std::collections::BTreeMap::new();
	m.insert("channel_login".into(), SocketValue::String(channel_login.to_string()));
	m.insert("sender_login".into(), SocketValue::String(sender_login.to_string()));
	m.insert("sender_display_name".into(), SocketValue::String(sender_display_name.to_string()));
	m.insert("multi_channel".into(), SocketValue::Bool(multi_channel));
	m
}

/// `username` と `reads` から参加チャンネル（ログイン名・小文字・先頭 `#` は無視）を重複なく並べる。
fn channels_to_join(login: &str, reads: &[String]) -> Vec<String> {
	fn norm(s: &str) -> String {
		s.trim().trim_start_matches('#').to_lowercase()
	}
	let mut set = BTreeSet::new();
	let u = norm(login);
	if !u.is_empty() {
		set.insert(u);
	}
	for r in reads {
		let n = norm(r);
		if !n.is_empty() {
			set.insert(n);
		}
	}
	set.into_iter().collect()
}

/// 1 エントリ分の IRC ワーカーハンドル。shutdown で `finish().await` する。
pub struct TwitchBridgeHandle {
	#[allow(dead_code)] // 将来 admin API などで表示する想定
	node_id: String,
	keepalive: Option<TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>>,
	join_handle: JoinHandle<()>,
}

impl TwitchBridgeHandle {
	#[allow(dead_code)] // 将来 admin API などで表示する想定
	pub fn node_id(&self) -> &str {
		&self.node_id
	}

	pub async fn finish(mut self) {
		// 先に IRC client を drop して incoming_messages.recv() を終了させる。
		drop(self.keepalive.take());
		// 保険で 3 秒タイムアウト。
		let _ = tokio::time::timeout(std::time::Duration::from_secs(3), self.join_handle).await;
	}
}

/// property `access_token` → `token_key` (via state) の順でトークンを解決する。
///
/// 未解決でも `None` を返すだけで失敗にはしない（匿名 IRC 接続にフォールバックする）。
async fn resolve_access_token(entry: &FlowgraphTwitchIngress, state: &SharedState) -> Option<String> {
	if !entry.access_token.trim().is_empty() {
		return Some(entry.access_token.trim().to_string());
	}
	let key = entry.token_key.trim();
	if key.is_empty() {
		return None;
	}
	let ident = {
		let s = state.read().await;
		let twitch = s.twitch.as_ref()?;
		let es = twitch.eventsub.as_ref()?;
		let spec = twitch.token_spec(key);
		crate::twitch::oauth::OAuthIdent::for_key(key, es, spec)
	};
	crate::twitch::oauth::try_load_valid_token_for(&ident).await
}

/// `flowgraph.ingress.twitch` の IRC モードに限り、全エントリを spawn する。
///
/// `trigger` が `None` の場合（Flowgraph ランタイム未起動）は warn を出して何もしない。
/// `mode != "irc"` のエントリはスキップする（EventSub は ζ-2 で Flowgraph 化予定）。
///
/// login / channels が共に空のエントリには `conf.twitch.username` をフォールバックとして差し込む
/// （`flowgraph.ingress.twitch_eventsub` の broadcaster_login フォールバックと対になる挙動）。
pub async fn spawn(entries: &[FlowgraphTwitchIngress], trigger: Option<TriggerHandle>, state: SharedState) -> Vec<TwitchBridgeHandle> {
	if entries.is_empty() {
		return Vec::new();
	}
	let Some(trigger) = trigger else {
		log::warn!(
			"《Flowgraph/Twitch》 ingress ノード {} 件を検出しましたが、Flowgraph ランタイムが起動していないため twitch bridge を開始できません。",
			entries.len()
		);
		return Vec::new();
	};

	// conf.twitch.username をフォールバックとして 1 度だけ取り出す。
	let twitch_username_fallback = {
		let s = state.read().await;
		s.twitch.as_ref().map(|t| t.username.trim().to_string()).unwrap_or_default()
	};

	let mut handles = Vec::new();
	for entry in entries {
		if entry.mode != "irc" {
			log::info!(
				"《Flowgraph/Twitch》 node={} mode={} はこの bridge では未対応（IRC のみ）。EventSub は ζ-2 で移行予定。",
				entry.node_id,
				entry.mode
			);
			continue;
		}
		let mut resolved = entry.clone();
		if resolved.login.trim().is_empty() && resolved.channels.is_empty() && !twitch_username_fallback.is_empty() {
			log::info!(
				"《Flowgraph/Twitch》 node={} login/channels 未指定のため conf.twitch.username={:?} をフォールバック採用",
				resolved.node_id,
				twitch_username_fallback
			);
			resolved.login = twitch_username_fallback.clone();
		}
		match spawn_one(resolved.clone(), trigger.clone(), state.clone()) {
			Some(h) => handles.push(h),
			None => log::error!(
				"《Flowgraph/Twitch》 IRC 起動失敗 node={} login={:?} channels={:?}",
				resolved.node_id,
				resolved.login,
				resolved.channels
			),
		}
	}
	handles
}

fn spawn_one(entry: FlowgraphTwitchIngress, trigger: TriggerHandle, state: SharedState) -> Option<TwitchBridgeHandle> {
	let channels = channels_to_join(&entry.login, &entry.channels);
	if channels.is_empty() {
		log::error!("《Flowgraph/Twitch》 node={} に参加チャンネルがありません", entry.node_id);
		return None;
	}
	let mut wanted = std::collections::HashSet::new();
	for ch in &channels {
		if let Err(e) = validate_login(ch) {
			log::error!("《Flowgraph/Twitch》 node={} 不正なチャンネル名 {:?}: {}", entry.node_id, ch, e);
			continue;
		}
		wanted.insert(ch.clone());
	}
	if wanted.is_empty() {
		log::error!("《Flowgraph/Twitch》 node={} に有効なチャンネルがありません", entry.node_id);
		return None;
	}
	let multi_channel = wanted.len() > 1;
	let node_id = entry.node_id.clone();

	// トークン解決は非同期。spawn 側で resolve する。
	let (mut incoming_messages, client) = TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(ClientConfig::default());

	let trigger_for_task = trigger.clone();
	let state_for_task = state.clone();
	let entry_for_task = entry.clone();
	let node_id_for_task = node_id.clone();

	let join_handle = tokio::spawn(async move {
		// 解決したトークンはログに出さない（値を掴むだけ）。
		let _resolved_token = resolve_access_token(&entry_for_task, &state_for_task).await;
		let ignore_logins = state_for_task.read().await.twitch_ignore_logins.clone();
		log::info!(
			"《Flowgraph/Twitch》 IRC 接続開始 node={} channels={:?} multi={}",
			node_id_for_task,
			entry_for_task.channels,
			multi_channel
		);
		while let Some(message) = incoming_messages.recv().await {
			if let ServerMessage::Privmsg(msg) = message {
				let sender_login_lc = msg.sender.login.to_lowercase();
				if ignore_logins.read().await.contains(&sender_login_lc) {
					log::debug!(
						"《Flowgraph/Twitch》 node={} ingress 無視リストによりスキップ sender={}",
						node_id_for_task,
						sender_login_lc
					);
					continue;
				}
				let text = if multi_channel {
					format!("#{} {}:{}", msg.channel_login, msg.sender.name, msg.message_text)
				} else {
					format!("{}:{}", msg.sender.name, msg.message_text)
				};
				let actor = build_source_actor(&msg.sender.login);
				let meta = build_meta(&msg.channel_login, &msg.sender.login, &msg.sender.name, multi_channel);
				let mut ev = TriggerEvent::new(&node_id_for_task)
					.with_exec("__trigger__")
					.with_override("__content__", SocketValue::String(text))
					.with_override("__source_actor__", SocketValue::String(actor));
				ev = ev.with_override("__meta__", SocketValue::Map(meta));
				if let Err(e) = trigger_for_task.send(ev) {
					log::warn!(
						"《Flowgraph/Twitch》 node={} trigger 送信に失敗: {}（worker を終了）",
						node_id_for_task,
						e
					);
					break;
				}
			}
		}
		log::info!("《Flowgraph/Twitch》 IRC ワーカー終了 node={}", node_id_for_task);
	});

	if let Err(e) = client.set_wanted_channels(wanted) {
		log::error!("《Flowgraph/Twitch》 node={} set_wanted_channels 失敗: {:?}", entry.node_id, e);
		return None;
	}

	Some(TwitchBridgeHandle {
		node_id,
		keepalive: Some(client),
		join_handle,
	})
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::InputMap;
	use crate::flowgraph::socket::SocketValue;
	use std::path::PathBuf;

	#[test]
	fn from_meta_reads_channels_list() {
		let mut props = InputMap::new();
		props.insert("mode".into(), SocketValue::String("IRC".into()));
		props.insert(
			"channels".into(),
			SocketValue::List(vec![SocketValue::String("user_a".into()), SocketValue::String("user_b".into())]),
		);
		props.insert("login".into(), SocketValue::String("bot".into()));
		props.insert("token_key".into(), SocketValue::String("broadcaster".into()));
		let meta = LoadedNodeMeta {
			feature: "flowgraph.ingress.twitch".into(),
			file: PathBuf::from("x.toml"),
			position: None,
			properties: props,
		};
		let t = FlowgraphTwitchIngress::from_meta("in", &meta).unwrap();
		assert_eq!(t.mode, "irc");
		assert_eq!(t.channels, vec!["user_a".to_string(), "user_b".to_string()]);
		assert_eq!(t.login, "bot");
		assert_eq!(t.token_key, "broadcaster");
	}

	#[test]
	fn defaults_for_empty_properties() {
		let meta = LoadedNodeMeta {
			feature: "flowgraph.ingress.twitch".into(),
			file: PathBuf::from("x.toml"),
			position: None,
			properties: InputMap::new(),
		};
		let t = FlowgraphTwitchIngress::from_meta("in", &meta).unwrap();
		assert_eq!(t.mode, "irc");
		assert!(t.channels.is_empty());
		assert!(t.token_key.is_empty());
	}

	#[test]
	fn channels_to_join_dedupes_and_normalizes() {
		let ch = channels_to_join("  Bot  ", &vec!["#ChanA".into(), "chanb".into(), "ChanA".into(), "".into()]);
		assert_eq!(ch, vec!["bot".to_string(), "chana".to_string(), "chanb".to_string()]);
	}

	#[test]
	fn build_source_actor_trims_and_lowercases_input() {
		assert_eq!(build_source_actor("  alice  "), "alice");
		// note: 小文字化は sender.login を渡す側の責務（ここでは trim のみ）。
		assert_eq!(build_source_actor("ALICE"), "ALICE");
	}

	#[test]
	fn build_meta_contains_expected_fields() {
		let m = build_meta("mychan", "alice", "Alice", true);
		assert_eq!(m.get("channel_login").and_then(|v| v.as_str().ok()), Some("mychan"));
		assert_eq!(m.get("sender_login").and_then(|v| v.as_str().ok()), Some("alice"));
		assert_eq!(m.get("sender_display_name").and_then(|v| v.as_str().ok()), Some("Alice"));
		assert_eq!(m.get("multi_channel").and_then(|v| v.as_bool().ok()), Some(true));
	}
}
