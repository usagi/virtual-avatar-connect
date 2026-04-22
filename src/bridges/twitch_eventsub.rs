//! `flowgraph.ingress.twitch_eventsub` 用ブリッジ（ζ-2）。
//!
//! ## 責務
//!
//! - `flowgraph.ingress.twitch_eventsub` ノードの property から
//!   [`FlowgraphTwitchEventsubIngress`] を生成する。
//! - [`spawn`] で各エントリに対し Twitch EventSub WebSocket セッションを張り、
//!   `notification` を受けるたびに `TriggerEvent` を `TriggerHandle::send` で流し込む。
//! - 返されるハンドルは shutdown 時に `finish().await` すること。
//!
//! ## V1 `src/twitch/eventsub.rs` との違い
//!
//! - V1 は `ChannelDatum` に人間可読な 1 行を詰めて `channel_datum_tx` に push する。
//!   Flowgraph からは `channel_subscribe` ingress 経由で再消費できるが、raw event JSON は **失われる**。
//! - 本 bridge は Flowgraph-native で raw event JSON を `payload` データ出力にそのまま流す。
//! - 両者は独立。`[twitch.eventsub].enabled = true` のまま `flowgraph.ingress.twitch_eventsub`
//!   を置くと WS が 2 本張られる点に注意（V1 側を落としたければ `enabled = false`）。
//!
//! ## トークン解決
//!
//! `token_key` (default `"broadcaster"`) から `OAuthIdent::for_key` で ident を組み、
//! `try_load_valid_token_for` で保存済みトークンを読む。未取得なら bridge 起動をスキップ（warn）。
//! V1 と違って DCF は自動で走らせない（設定画面 / CLI で事前に `key` ごとに 1 度通す想定、ζ-1 の方針）。

use crate::flowgraph::loader::LoadedNodeMeta;
use crate::flowgraph::node::{TriggerEvent, TriggerHandle};
use crate::flowgraph::socket::SocketValue;
use crate::SharedState;
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

/// 1 つの `flowgraph.ingress.twitch_eventsub` ノードの設定スナップショット。
#[derive(Debug, Clone)]
pub struct FlowgraphTwitchEventsubIngress {
	pub node_id: String,
	pub token_key: String,
	pub broadcaster_login: String,
	pub event_types: Vec<String>,
	pub channel_points_reward_id: String,
	#[allow(dead_code)] // echo 用途で将来使う予定
	pub fixed_channel: String,
}

impl FlowgraphTwitchEventsubIngress {
	pub fn from_meta(fq: &str, meta: &LoadedNodeMeta) -> Option<Self> {
		let p = &meta.properties;
		let get_str = |k: &str| -> String {
			p.get(k)
				.and_then(|v| v.as_str().ok())
				.map(str::trim)
				.map(str::to_string)
				.unwrap_or_default()
		};
		let event_types: Vec<String> = p
			.get("event_types")
			.and_then(|v| v.as_list().ok())
			.map(|items| {
				items
					.iter()
					.filter_map(|v| v.as_str().ok().map(|s| s.trim().to_string()))
					.filter(|s| !s.is_empty())
					.collect()
			})
			.unwrap_or_default();
		let token_key = {
			let k = get_str("token_key");
			if k.is_empty() { "broadcaster".into() } else { k }
		};
		Some(Self {
			node_id: fq.to_string(),
			token_key,
			broadcaster_login: get_str("broadcaster_login"),
			event_types,
			channel_points_reward_id: get_str("channel_points_reward_id"),
			fixed_channel: get_str("fixed_channel"),
		})
	}
}

/// ingress エントリから **正規化済み broadcaster_login**（小文字・trim・先頭 `#` 除去）を
/// `conf.twitch.username` のフォールバックを効かせて解決する。
///
/// ζ-2c: V1 `spawn_eventsub_loop` を自動スキップする判定で使う。trigger / token など
/// 実際の起動資材に触れず、純粋に設定名だけを返すので同期的。
///
/// 空文字しか解決できないときは `None` を返す。
pub fn normalized_broadcaster_login(
	entry: &FlowgraphTwitchEventsubIngress,
	twitch_username_fallback: &str,
) -> Option<String> {
	fn norm(s: &str) -> String {
		s.trim().trim_start_matches('#').to_lowercase()
	}
	let from_prop = norm(&entry.broadcaster_login);
	if !from_prop.is_empty() {
		return Some(from_prop);
	}
	let from_conf = norm(twitch_username_fallback);
	if !from_conf.is_empty() {
		return Some(from_conf);
	}
	None
}

/// ζ-2c: 与えられた ingress エントリ列から、V1 eventsub_loop をスキップすべき
/// broadcaster_login 集合（正規化済み）を返す。
pub fn v1_skip_broadcaster_logins(
	entries: &[FlowgraphTwitchEventsubIngress],
	twitch_username_fallback: &str,
) -> std::collections::BTreeSet<String> {
	let mut set = std::collections::BTreeSet::new();
	for e in entries {
		if let Some(bl) = normalized_broadcaster_login(e, twitch_username_fallback) {
			set.insert(bl);
		}
	}
	set
}

/// 1 つの WS セッションを保持するハンドル。`finish().await` で graceful shutdown。
pub struct TwitchEventsubBridgeHandle {
	#[allow(dead_code)] // 将来 admin API などで表示する想定
	node_id: String,
	shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
	join_handle: JoinHandle<()>,
}

impl TwitchEventsubBridgeHandle {
	#[allow(dead_code)]
	pub fn node_id(&self) -> &str {
		&self.node_id
	}

	pub async fn finish(mut self) {
		if let Some(tx) = self.shutdown_tx.take() {
			let _ = tx.send(true);
		}
		let _ = tokio::time::timeout(Duration::from_secs(3), self.join_handle).await;
	}
}

/// ブリッジ資材（トークン / client_id / broadcaster_login / broadcaster_id）を `SharedState`
/// から解決する。token / broadcaster_login のどちらが不足しても `Err` を返す。
async fn resolve_resources(
	entry: &FlowgraphTwitchEventsubIngress,
	state: &SharedState,
) -> Result<ResolvedResources> {
	let s = state.read().await;
	let twitch = s
		.twitch
		.as_ref()
		.ok_or_else(|| anyhow!("[twitch] section is not configured"))?;
	let es = twitch
		.eventsub
		.as_ref()
		.ok_or_else(|| anyhow!("[twitch.eventsub] section is not configured"))?;
	let spec = twitch.token_spec(&entry.token_key);
	let ident = crate::twitch::oauth::OAuthIdent::for_key(&entry.token_key, es, spec);
	let client_id = ident.client_id.clone();
	drop(s);
	if client_id.is_empty() {
		return Err(anyhow!(
			"twitch client_id が空です（VAC_TWITCH_CLIENT_ID または [twitch.eventsub].client_id を設定してください）"
		));
	}
	let access_token = crate::twitch::oauth::try_load_valid_token_for(&ident)
		.await
		.ok_or_else(|| {
			anyhow!(
				"token_key='{}' の有効な保存済みトークンがありません（設定画面で DCF を通してください）",
				entry.token_key
			)
		})?;

	// broadcaster_login: property > conf.twitch.username > 空
	let broadcaster_login = {
		let s = state.read().await;
		let twitch = s.twitch.as_ref().unwrap();
		if !entry.broadcaster_login.is_empty() {
			entry.broadcaster_login.clone()
		} else if !twitch.username.trim().is_empty() {
			twitch.username.trim().to_lowercase()
		} else {
			String::new()
		}
	};
	if broadcaster_login.is_empty() {
		return Err(anyhow!(
			"broadcaster_login が解決できません（node property または conf.twitch.username を設定してください）"
		));
	}

	// broadcaster_id を Helix で引く
	let broadcaster_id = crate::twitch::eventsub::helix_user_id(&client_id, &access_token, &broadcaster_login)
		.await
		.map_err(|e| anyhow!("helix_user_id 失敗 ({}): {}", broadcaster_login, e))?;

	// EventSub 設定（subscription 既定に使う）。bridge オーナーだけが所有する clone。
	let es_cfg = {
		let s = state.read().await;
		s.twitch.as_ref().unwrap().eventsub.clone().unwrap()
	};
	Ok(ResolvedResources {
		client_id,
		access_token,
		broadcaster_login,
		broadcaster_id,
		es_cfg,
	})
}

struct ResolvedResources {
	client_id: String,
	access_token: String,
	broadcaster_login: String,
	broadcaster_id: String,
	es_cfg: crate::conf::TwitchEventSubConfig,
}

/// 購読キュー（sub_type, version, condition）を組み立てる。
///
/// - `event_types` が非空 → 明示指定を使う（channel_points は `channel_points_reward_id` で展開、
///   空なら自動列挙）。
/// - 空 → `conf.twitch.eventsub` の bool トグルから既定セットを使う（V1 同等）。
async fn build_subscription_queue(
	entry: &FlowgraphTwitchEventsubIngress,
	res: &ResolvedResources,
) -> Result<Vec<(String, String, Value)>> {
	let mut out: Vec<(String, String, Value)> = Vec::new();

	if !entry.event_types.is_empty() {
		// 明示: event_types プロパティ主導
		let has_points = entry
			.event_types
			.iter()
			.any(|s| s.trim() == "channel.channel_points_custom_reward_redemption.add");
		for (t, v, cond) in crate::twitch::eventsub::explicit_subscription_queue(&entry.event_types, &res.broadcaster_id) {
			if t == "channel.channel_points_custom_reward_redemption.add" {
				continue; // 下で個別処理
			}
			out.push((t, v.to_string(), cond));
		}
		if has_points {
			let reward_ids = if !entry.channel_points_reward_id.trim().is_empty() {
				vec![entry.channel_points_reward_id.trim().to_string()]
			} else {
				crate::twitch::eventsub::helix_list_custom_reward_ids(
					&res.client_id,
					&res.access_token,
					&res.broadcaster_id,
				)
				.await?
			};
			for rid in reward_ids {
				out.push((
					"channel.channel_points_custom_reward_redemption.add".into(),
					"1".into(),
					json!({
						"broadcaster_user_id": res.broadcaster_id,
						"reward_id": rid,
					}),
				));
			}
		}
	} else {
		// 既定: conf.twitch.eventsub の bool を踏襲
		for (t, v, cond) in
			crate::twitch::eventsub::default_subscription_queue_from_cfg(&res.es_cfg, &res.broadcaster_id)
		{
			out.push((t.to_string(), v.to_string(), cond));
		}
		// channel_points は V1 と同じルール（reward_id 指定 / 空なら自動列挙）
		if res.es_cfg.channel_points {
			let reward_ids = match &res.es_cfg.channel_points_reward_id {
				Some(rid) if !rid.trim().is_empty() => vec![rid.trim().to_string()],
				_ => {
					crate::twitch::eventsub::helix_list_custom_reward_ids(
						&res.client_id,
						&res.access_token,
						&res.broadcaster_id,
					)
					.await?
				},
			};
			for rid in reward_ids {
				out.push((
					"channel.channel_points_custom_reward_redemption.add".into(),
					"1".into(),
					json!({
						"broadcaster_user_id": res.broadcaster_id,
						"reward_id": rid,
					}),
				));
			}
		}
	}
	Ok(out)
}

/// 名前候補から空でない最初の値を拾う。
fn pick_str<'a>(event: &'a Value, keys: &[&str]) -> Option<&'a str> {
	for k in keys {
		if let Some(s) = event.get(*k).and_then(|v| v.as_str()) {
			if !s.is_empty() {
				return Some(s);
			}
		}
	}
	None
}

/// `sub_type` に応じてアクター表示を決める（V1 の `build_notification_datum` と同じルール）。
fn extract_actor(sub_type: &str, event: &Value) -> (String, String) {
	let (name_keys, login_keys): (&[&str], &[&str]) = match sub_type {
		"channel.raid" => (
			&["from_broadcaster_user_name"],
			&["from_broadcaster_user_login"],
		),
		"stream.online" | "stream.offline" => (&["broadcaster_user_name"], &["broadcaster_user_login"]),
		"channel.subscription.gift" => {
			if event.get("is_anonymous").and_then(|v| v.as_bool()) == Some(true) {
				return ("匿名".into(), String::new());
			}
			(&["user_name"], &["user_login"])
		},
		_ => (&["user_name"], &["user_login"]),
	};
	let name = pick_str(event, name_keys).unwrap_or_default().to_string();
	let login = pick_str(event, login_keys).unwrap_or_default().to_string();
	(name, login)
}

/// `meta` に詰める補助情報。raw event JSON は別途 `payload` 出力で流すので、
/// ここではサブスクリプション文脈（どの broadcaster / どの node 由来か）だけを入れる。
fn build_meta_map(
	broadcaster_login: &str,
	broadcaster_id: &str,
	node_id: &str,
	sub_type: &str,
	sub_version: &str,
) -> BTreeMap<String, SocketValue> {
	let mut m = BTreeMap::new();
	m.insert("broadcaster_login".into(), SocketValue::String(broadcaster_login.to_string()));
	m.insert("broadcaster_id".into(), SocketValue::String(broadcaster_id.to_string()));
	m.insert("ingress_node_id".into(), SocketValue::String(node_id.to_string()));
	m.insert("event_type".into(), SocketValue::String(sub_type.to_string()));
	m.insert("sub_version".into(), SocketValue::String(sub_version.to_string()));
	m
}

/// `flowgraph.ingress.twitch_eventsub` ノードを全件 spawn する。
///
/// - `trigger` が `None` の場合（Flowgraph ランタイム未起動）は warn を出して何もしない。
/// - 各エントリは独立した tokio タスクで WS を張り、5 秒スリープ付きの reconnect ループで回る。
pub fn spawn(
	entries: &[FlowgraphTwitchEventsubIngress],
	trigger: Option<TriggerHandle>,
	state: SharedState,
) -> Vec<TwitchEventsubBridgeHandle> {
	if entries.is_empty() {
		return Vec::new();
	}
	let Some(trigger) = trigger else {
		log::warn!(
			"《Flowgraph/EventSub》 ingress ノード {} 件を検出しましたが、Flowgraph ランタイムが起動していないため bridge を開始できません。",
			entries.len()
		);
		return Vec::new();
	};
	let mut handles = Vec::new();
	for entry in entries {
		let handle = spawn_one(entry.clone(), trigger.clone(), state.clone());
		handles.push(handle);
	}
	handles
}

fn spawn_one(
	entry: FlowgraphTwitchEventsubIngress,
	trigger: TriggerHandle,
	state: SharedState,
) -> TwitchEventsubBridgeHandle {
	let node_id = entry.node_id.clone();
	let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
	let trigger_for_task = trigger.clone();
	let state_for_task = state.clone();
	let entry_for_task = entry.clone();
	let node_id_for_task = node_id.clone();

	let join_handle = tokio::spawn(async move {
		log::info!(
			"《Flowgraph/EventSub》 ingress 起動 node={} token_key={} broadcaster={:?}",
			node_id_for_task,
			entry_for_task.token_key,
			entry_for_task.broadcaster_login
		);
		loop {
			if *shutdown_rx.borrow() {
				break;
			}
			match session_loop(&entry_for_task, &state_for_task, &trigger_for_task, &mut shutdown_rx).await {
				Ok(()) => log::info!("《Flowgraph/EventSub》 session 終了 node={}（再接続します）", node_id_for_task),
				Err(e) => log::error!("《Flowgraph/EventSub》 node={}: {:?}", node_id_for_task, e),
			}
			// shutdown が立っていれば即抜け、そうでなければ 5 秒待って reconnect
			tokio::select! {
				_ = tokio::time::sleep(Duration::from_secs(5)) => {},
				_ = shutdown_rx.changed() => {},
			}
		}
		log::info!("《Flowgraph/EventSub》 ingress 停止 node={}", node_id_for_task);
	});

	TwitchEventsubBridgeHandle {
		node_id,
		shutdown_tx: Some(shutdown_tx),
		join_handle,
	}
}

async fn session_loop(
	entry: &FlowgraphTwitchEventsubIngress,
	state: &SharedState,
	trigger: &TriggerHandle,
	shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
	let res = resolve_resources(entry, state).await?;
	log::info!(
		"《Flowgraph/EventSub》 node={} broadcaster_login={} broadcaster_id={}",
		entry.node_id,
		res.broadcaster_login,
		res.broadcaster_id
	);

	let (mut ws_stream, _) = tokio_tungstenite::connect_async(crate::twitch::eventsub::EVENTSUB_WS)
		.await
		.map_err(|e| anyhow!("EventSub WebSocket 接続失敗: {}", e))?;

	// session_welcome を待つ
	let welcome_str: String = loop {
		let next = tokio::time::timeout(Duration::from_secs(15), ws_stream.next())
			.await
			.map_err(|_| anyhow!("EventSub: session_welcome 受信タイムアウト (15s)"))?
			.ok_or_else(|| anyhow!("EventSub: 最初のメッセージがありません"))??;
		match next {
			Message::Text(s) => break s.as_str().to_string(),
			Message::Close(frame) => {
				return Err(anyhow!("EventSub: welcome 前に Close されました: {:?}", frame));
			},
			_ => continue,
		}
	};
	let session_id = crate::twitch::eventsub::parse_session_welcome(&welcome_str)?;
	log::debug!("《Flowgraph/EventSub》 node={} session_id={}", entry.node_id, session_id);

	// WS 読み取りループを別タスクで（Helix 購読作成と auto-pong を並行させる）
	let node_id_for_read = entry.node_id.clone();
	let broadcaster_login_for_read = res.broadcaster_login.clone();
	let broadcaster_id_for_read = res.broadcaster_id.clone();
	let trigger_for_read = trigger.clone();
	let mut shutdown_rx_for_read = shutdown_rx.clone();
	let read_task: JoinHandle<Result<()>> = tokio::spawn(async move {
		let mut ws = ws_stream;
		loop {
			tokio::select! {
				_ = shutdown_rx_for_read.changed() => {
					if *shutdown_rx_for_read.borrow() {
						return Ok(());
					}
				}
				maybe_msg = ws.next() => {
					let Some(msg) = maybe_msg else {
						return Ok(());
					};
					match msg? {
						Message::Text(t) => {
							if let Err(e) = handle_notification(
								t.as_str(),
								&node_id_for_read,
								&broadcaster_login_for_read,
								&broadcaster_id_for_read,
								&trigger_for_read,
							) {
								log::warn!("《Flowgraph/EventSub》 handle_notification: {}", e);
							}
						}
						Message::Close(frame) => {
							log::info!("《Flowgraph/EventSub》 node={} サーバから Close: {:?}", node_id_for_read, frame);
							return Ok(());
						}
						_ => continue,
					}
				}
			}
		}
	});

	// 購読作成
	let queue = build_subscription_queue(entry, &res).await?;
	if queue.is_empty() {
		log::warn!(
			"《Flowgraph/EventSub》 node={} 購読対象が 0 件です（event_types か conf.twitch.eventsub のトグルを見直してください）",
			entry.node_id
		);
	}
	for (sub_type, version, cond) in queue.iter() {
		if let Err(e) = crate::twitch::eventsub::post_subscription(
			&res.client_id,
			&res.access_token,
			&session_id,
			sub_type,
			version,
			cond,
		)
		.await
		{
			read_task.abort();
			return Err(e);
		}
	}

	// 読み取りタスクの完了を待つ（Close or shutdown）
	match read_task.await {
		Ok(Ok(())) => Ok(()),
		Ok(Err(e)) => Err(e),
		Err(join_err) if join_err.is_cancelled() => Ok(()),
		Err(join_err) => Err(anyhow!("read task panic: {}", join_err)),
	}
}

/// 1 本の EventSub WS テキストメッセージを処理。`notification` なら TriggerEvent を発火する。
fn handle_notification(
	text: &str,
	node_id: &str,
	broadcaster_login: &str,
	broadcaster_id: &str,
	trigger: &TriggerHandle,
) -> Result<()> {
	let v: Value = serde_json::from_str(text)?;
	let msg_type = v["metadata"]["message_type"].as_str().unwrap_or("");
	match msg_type {
		"session_keepalive" => return Ok(()),
		"session_reconnect" => {
			let url = v["payload"]["session"]["reconnect_url"].as_str().unwrap_or("");
			log::warn!(
				"《Flowgraph/EventSub》 node={} session_reconnect url={}（bridge は session 終了後に reconnect します）",
				node_id,
				url
			);
			return Err(anyhow!("reconnect"));
		},
		"revocation" => {
			log::warn!("《Flowgraph/EventSub》 node={} revocation: {:?}", node_id, v);
			return Ok(());
		},
		"notification" => {},
		other => {
			log::trace!("《Flowgraph/EventSub》 node={} unknown message type: {}", node_id, other);
			return Ok(());
		},
	}

	let sub_type = v["payload"]["subscription"]["type"].as_str().unwrap_or("").to_string();
	let sub_version = v["payload"]["subscription"]["version"].as_str().unwrap_or("").to_string();
	let event = v["payload"]["event"].clone();
	let (actor_name, actor_login) = extract_actor(&sub_type, &event);
	let meta = build_meta_map(
		broadcaster_login,
		broadcaster_id,
		node_id,
		&sub_type,
		&sub_version,
	);

	let ev = TriggerEvent::new(node_id)
		.with_exec("__trigger__")
		.with_override("__event_type__", SocketValue::String(sub_type))
		.with_override("__actor_name__", SocketValue::String(actor_name))
		.with_override("__actor_login__", SocketValue::String(actor_login))
		.with_override(
			"__broadcaster_login__",
			SocketValue::String(broadcaster_login.to_string()),
		)
		.with_override("__payload__", SocketValue::Json(event))
		.with_override("__meta__", SocketValue::Map(meta));
	trigger
		.send(ev)
		.map_err(|e| anyhow!("TriggerHandle::send failed: {}", e))?;
	Ok(())
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::InputMap;
	use std::path::PathBuf;

	fn meta_with(props: InputMap) -> LoadedNodeMeta {
		LoadedNodeMeta {
			feature: "flowgraph.ingress.twitch_eventsub".into(),
			file: PathBuf::from("x.toml"),
			position: None,
			properties: props,
		}
	}

	#[test]
	fn from_meta_reads_all_props() {
		let mut props = InputMap::new();
		props.insert("token_key".into(), SocketValue::String("moderator".into()));
		props.insert("broadcaster_login".into(), SocketValue::String("Alice".into()));
		props.insert(
			"event_types".into(),
			SocketValue::List(vec![
				SocketValue::String("channel.cheer".into()),
				SocketValue::String(" channel.raid ".into()),
			]),
		);
		props.insert(
			"channel_points_reward_id".into(),
			SocketValue::String("uuid-0001".into()),
		);
		let t = FlowgraphTwitchEventsubIngress::from_meta("in", &meta_with(props)).unwrap();
		assert_eq!(t.token_key, "moderator");
		assert_eq!(t.broadcaster_login, "Alice");
		assert_eq!(t.event_types, vec!["channel.cheer".to_string(), "channel.raid".to_string()]);
		assert_eq!(t.channel_points_reward_id, "uuid-0001");
	}

	#[test]
	fn defaults_token_key_is_broadcaster() {
		let t = FlowgraphTwitchEventsubIngress::from_meta("in", &meta_with(InputMap::new())).unwrap();
		assert_eq!(t.token_key, "broadcaster");
		assert!(t.event_types.is_empty());
	}

	#[test]
	fn extract_actor_picks_user_for_default_events() {
		let ev = json!({"user_name": "Bob", "user_login": "bob"});
		let (n, l) = extract_actor("channel.cheer", &ev);
		assert_eq!(n, "Bob");
		assert_eq!(l, "bob");
	}

	#[test]
	fn extract_actor_uses_from_user_for_raid() {
		let ev = json!({
			"from_broadcaster_user_name": "Carol",
			"from_broadcaster_user_login": "carol",
		});
		let (n, l) = extract_actor("channel.raid", &ev);
		assert_eq!(n, "Carol");
		assert_eq!(l, "carol");
	}

	#[test]
	fn extract_actor_uses_broadcaster_for_stream_online() {
		let ev = json!({"broadcaster_user_name": "Dave", "broadcaster_user_login": "dave"});
		let (n, l) = extract_actor("stream.online", &ev);
		assert_eq!(n, "Dave");
		assert_eq!(l, "dave");
	}

	#[test]
	fn extract_actor_anonymous_gift() {
		let ev = json!({"is_anonymous": true, "user_name": "should_ignore"});
		let (n, l) = extract_actor("channel.subscription.gift", &ev);
		assert_eq!(n, "匿名");
		assert_eq!(l, "");
	}

	#[test]
	fn normalize_prefers_property_over_conf() {
		let mut props = InputMap::new();
		props.insert("broadcaster_login".into(), SocketValue::String(" #Alice ".into()));
		let t = FlowgraphTwitchEventsubIngress::from_meta("in", &meta_with(props)).unwrap();
		assert_eq!(
			normalized_broadcaster_login(&t, "fallback_user"),
			Some("alice".to_string())
		);
	}

	#[test]
	fn normalize_falls_back_to_conf_username() {
		let t = FlowgraphTwitchEventsubIngress::from_meta("in", &meta_with(InputMap::new())).unwrap();
		assert_eq!(
			normalized_broadcaster_login(&t, "  #BobBot  "),
			Some("bobbot".to_string())
		);
	}

	#[test]
	fn normalize_returns_none_when_both_empty() {
		let t = FlowgraphTwitchEventsubIngress::from_meta("in", &meta_with(InputMap::new())).unwrap();
		assert_eq!(normalized_broadcaster_login(&t, ""), None);
	}

	#[test]
	fn v1_skip_set_collects_all_entries() {
		let mk = |bl: &str| {
			let mut props = InputMap::new();
			if !bl.is_empty() {
				props.insert("broadcaster_login".into(), SocketValue::String(bl.into()));
			}
			FlowgraphTwitchEventsubIngress::from_meta("x", &meta_with(props)).unwrap()
		};
		let entries = vec![mk("Alice"), mk(""), mk("carol")];
		let set = v1_skip_broadcaster_logins(&entries, "streamer_bob");
		assert!(set.contains("alice"));
		assert!(set.contains("streamer_bob"));
		assert!(set.contains("carol"));
		assert_eq!(set.len(), 3);
	}

	#[test]
	fn build_meta_map_contains_expected_fields() {
		let m = build_meta_map("mychan", "111", "node1", "channel.cheer", "1");
		assert_eq!(m.get("broadcaster_login").and_then(|v| v.as_str().ok()), Some("mychan"));
		assert_eq!(m.get("broadcaster_id").and_then(|v| v.as_str().ok()), Some("111"));
		assert_eq!(m.get("ingress_node_id").and_then(|v| v.as_str().ok()), Some("node1"));
		assert_eq!(m.get("event_type").and_then(|v| v.as_str().ok()), Some("channel.cheer"));
		assert_eq!(m.get("sub_version").and_then(|v| v.as_str().ok()), Some("1"));
	}
}
