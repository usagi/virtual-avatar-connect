//! `flowgraph.ingress.channel_subscribe` 用ブリッジ（δ-9 Part E）。
//!
//! V1 `State.channel_data` への `push_channel_datum` / `push_channel_datum_quiet` /
//! `finalize_channel_datum_and_dispatch` すべてが `State.channel_datum_tx` broadcast に
//! `ChannelDatum` を流している。本ブリッジはそれを subscribe し、ingress ノード単位の
//! フィルタを通して `TriggerEvent` を Flowgraph ワーカーに投入する。
//!
//! ## 設計ポイント
//!
//! - 1 ingress ノード = 1 tokio::task。`channel_datum_tx.subscribe()` で受信 → フィルタ適用 →
//!   `TriggerHandle::send` の 1 本パス。
//! - エコー防止は 2 段:
//!   1. `ignore_flowgraph_echo = true`（既定）: meta.source_actor が `flowgraph` / `flowgraph:*` の datum を drop
//!   2. `ignore_source_actors = [...]`: 明示的な actor 名マッチで drop（bot login エコー等）
//! - `require_final` は既定 true。`push_channel_datum_quiet` 由来の streaming 中間は通さない。
//! - Broadcast の `Lagged(n)` は warn ログのみで飲み込む（subscribe 続行）。

use crate::flowgraph::loader::LoadedNodeMeta;
use crate::flowgraph::node::{TriggerEvent, TriggerHandle};
use crate::flowgraph::socket::SocketValue;
use crate::state::ChannelDatum;
use std::collections::HashSet;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct FlowgraphChannelSubscribe {
	pub node_id: String,
	pub channels: HashSet<String>,
	pub require_final: bool,
	pub ignore_source_actors: HashSet<String>,
	pub ignore_flowgraph_echo: bool,
	pub require_flags: HashSet<String>,
	pub drop_flags: HashSet<String>,
}

impl FlowgraphChannelSubscribe {
	pub fn from_meta(fq: &str, meta: &LoadedNodeMeta) -> Option<Self> {
		let p = &meta.properties;
		let get_string_list = |k: &str| -> HashSet<String> {
			p.get(k)
				.and_then(|v| match v {
					SocketValue::List(xs) => Some(xs),
					_ => None,
				})
				.map(|xs| {
					xs.iter()
						.filter_map(|v| match v {
							SocketValue::String(s) => {
								let t = s.trim();
								if t.is_empty() {
									None
								} else {
									Some(t.to_string())
								}
							}
							_ => None,
						})
						.collect()
				})
				.unwrap_or_default()
		};
		let get_bool = |k: &str, default: bool| -> bool {
			p.get(k).and_then(|v| v.as_bool().ok()).unwrap_or(default)
		};
		Some(Self {
			node_id: fq.to_string(),
			channels: get_string_list("channels"),
			require_final: get_bool("require_final", true),
			ignore_source_actors: get_string_list("ignore_source_actors"),
			ignore_flowgraph_echo: get_bool("ignore_flowgraph_echo", true),
			require_flags: get_string_list("require_flags"),
			drop_flags: get_string_list("drop_flags"),
		})
	}

	/// 1 件の `ChannelDatum` を本 ingress が流してよいか判定。
	pub fn matches(&self, cd: &ChannelDatum) -> bool {
		if !self.channels.is_empty() && !self.channels.contains(&cd.channel) {
			return false;
		}
		if self.require_final && !cd.has_flag(ChannelDatum::FLAG_IS_FINAL) {
			return false;
		}
		for f in &self.drop_flags {
			if cd.has_flag(f) {
				return false;
			}
		}
		for f in &self.require_flags {
			if !cd.has_flag(f) {
				return false;
			}
		}
		let actor = extract_source_actor(cd).unwrap_or_default();
		if self.ignore_flowgraph_echo {
			if actor == "flowgraph" || actor.starts_with("flowgraph:") {
				return false;
			}
		}
		if !actor.is_empty() && self.ignore_source_actors.contains(&actor) {
			return false;
		}
		true
	}

	/// ingress ノードのポート overrides に整形。
	pub fn build_trigger(&self, cd: &ChannelDatum) -> TriggerEvent {
		let meta_json: serde_json::Value = serde_json::to_value(&cd.meta).unwrap_or(serde_json::Value::Null);
		let actor = extract_source_actor(cd).unwrap_or_default();
		let is_final = cd.has_flag(ChannelDatum::FLAG_IS_FINAL);
		TriggerEvent::new(self.node_id.clone())
			.with_exec("__trigger__")
			.with_override("__channel__", SocketValue::String(cd.channel.clone()))
			.with_override("__content__", SocketValue::String(cd.content.clone()))
			.with_override("__source_actor__", SocketValue::String(actor))
			.with_override("__is_final__", SocketValue::Bool(is_final))
			.with_override("__meta__", SocketValue::Json(meta_json))
	}
}

fn extract_source_actor(cd: &ChannelDatum) -> Option<String> {
	cd.meta
		.get("source_actor")
		.and_then(|v| v.as_str())
		.map(str::trim)
		.filter(|s| !s.is_empty())
		.map(|s| s.to_string())
}

/// channel.subscribe ingress を全部 spawn する。
///
/// - `entries` が空 or `trigger` が None 相当ならログのみで何もしない。
/// - それぞれ独立した `tokio::task` で broadcast を待ち、マッチ時に `TriggerHandle::send`。
/// - `channel_datum_rx` の `subscribe()` は Program 停止後もドロップされて自動終了する。
pub fn spawn(
	entries: &[FlowgraphChannelSubscribe],
	trigger: Option<TriggerHandle>,
	channel_datum_tx: &broadcast::Sender<ChannelDatum>,
) -> Vec<tokio::task::JoinHandle<()>> {
	if entries.is_empty() {
		return Vec::new();
	}
	let Some(trigger) = trigger else {
		log::warn!(
			"《Flowgraph/ChannelSubscribe》 ingress ノード {} 件を検出しましたが、Flowgraph worker が \
			 未起動のため bridge は起動しません。",
			entries.len()
		);
		return Vec::new();
	};
	let mut handles = Vec::with_capacity(entries.len());
	for sub in entries {
		let sub = sub.clone();
		let trigger = trigger.clone();
		let mut rx = channel_datum_tx.subscribe();
		log::info!(
			"《Flowgraph/ChannelSubscribe》 node={} channels={:?} require_final={} ignore_flowgraph_echo={}",
			sub.node_id,
			sub.channels,
			sub.require_final,
			sub.ignore_flowgraph_echo
		);
		let h = tokio::spawn(async move {
			loop {
				match rx.recv().await {
					Ok(cd) => {
						if !sub.matches(&cd) {
							continue;
						}
						let ev = sub.build_trigger(&cd);
						if let Err(e) = trigger.send(ev) {
							log::warn!(
								"《Flowgraph/ChannelSubscribe》 node={} trigger.send 失敗: {:?}",
								sub.node_id,
								e
							);
						}
					}
					Err(broadcast::error::RecvError::Lagged(n)) => {
						log::warn!(
							"《Flowgraph/ChannelSubscribe》 node={} channel_datum broadcast が {} 件 lag しました（購読続行）",
							sub.node_id,
							n
						);
					}
					Err(broadcast::error::RecvError::Closed) => {
						log::debug!(
							"《Flowgraph/ChannelSubscribe》 node={} channel_datum_tx closed: タスク終了",
							sub.node_id
						);
						return;
					}
				}
			}
		});
		handles.push(h);
	}
	handles
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
			feature: "flowgraph.ingress.channel_subscribe".into(),
			file: PathBuf::from("x.toml"),
			position: None,
			properties: props,
		}
	}

	#[test]
	fn from_meta_defaults() {
		let sub = FlowgraphChannelSubscribe::from_meta("n", &meta_with(InputMap::new())).unwrap();
		assert!(sub.channels.is_empty());
		assert!(sub.require_final);
		assert!(sub.ignore_flowgraph_echo);
	}

	#[test]
	fn from_meta_reads_channels_and_flags() {
		let mut p = InputMap::new();
		p.insert(
			"channels".into(),
			SocketValue::List(vec![SocketValue::String("user".into()), SocketValue::String("ai".into())]),
		);
		p.insert("require_final".into(), SocketValue::Bool(false));
		p.insert("ignore_flowgraph_echo".into(), SocketValue::Bool(false));
		p.insert(
			"ignore_source_actors".into(),
			SocketValue::List(vec![SocketValue::String("usaginetwork".into())]),
		);
		let sub = FlowgraphChannelSubscribe::from_meta("n", &meta_with(p)).unwrap();
		assert!(sub.channels.contains("user"));
		assert!(sub.channels.contains("ai"));
		assert!(!sub.require_final);
		assert!(!sub.ignore_flowgraph_echo);
		assert!(sub.ignore_source_actors.contains("usaginetwork"));
	}

	#[test]
	fn matches_filters_by_channel() {
		let mut p = InputMap::new();
		p.insert(
			"channels".into(),
			SocketValue::List(vec![SocketValue::String("user".into())]),
		);
		p.insert("require_final".into(), SocketValue::Bool(false));
		let sub = FlowgraphChannelSubscribe::from_meta("n", &meta_with(p)).unwrap();
		let on_user = ChannelDatum::new("user".into(), "hi".into());
		let on_ai = ChannelDatum::new("ai".into(), "hi".into());
		assert!(sub.matches(&on_user));
		assert!(!sub.matches(&on_ai));
	}

	#[test]
	fn matches_respects_require_final() {
		let sub = FlowgraphChannelSubscribe::from_meta("n", &meta_with(InputMap::new())).unwrap();
		let not_final = ChannelDatum::new("user".into(), "x".into());
		let finalized = ChannelDatum::new("user".into(), "x".into()).with_flag(ChannelDatum::FLAG_IS_FINAL);
		assert!(!sub.matches(&not_final));
		assert!(sub.matches(&finalized));
	}

	#[test]
	fn matches_ignores_flowgraph_echo_by_default() {
		let mut p = InputMap::new();
		p.insert("require_final".into(), SocketValue::Bool(false));
		let sub = FlowgraphChannelSubscribe::from_meta("n", &meta_with(p)).unwrap();
		let echoed = ChannelDatum::new("ai".into(), "x".into())
			.with_meta("source_actor", "flowgraph:emit_ai");
		assert!(!sub.matches(&echoed));
		let external = ChannelDatum::new("ai".into(), "x".into())
			.with_meta("source_actor", "user");
		assert!(sub.matches(&external));
	}

	#[test]
	fn matches_ignores_source_actors_list() {
		let mut p = InputMap::new();
		p.insert("require_final".into(), SocketValue::Bool(false));
		p.insert(
			"ignore_source_actors".into(),
			SocketValue::List(vec![SocketValue::String("bot1".into())]),
		);
		let sub = FlowgraphChannelSubscribe::from_meta("n", &meta_with(p)).unwrap();
		let from_bot = ChannelDatum::new("chat".into(), "x".into()).with_meta("source_actor", "bot1");
		let from_other = ChannelDatum::new("chat".into(), "x".into()).with_meta("source_actor", "alice");
		assert!(!sub.matches(&from_bot));
		assert!(sub.matches(&from_other));
	}

	#[test]
	fn matches_respects_require_and_drop_flags() {
		let mut p = InputMap::new();
		p.insert("require_final".into(), SocketValue::Bool(false));
		p.insert(
			"require_flags".into(),
			SocketValue::List(vec![SocketValue::String("needed".into())]),
		);
		p.insert(
			"drop_flags".into(),
			SocketValue::List(vec![SocketValue::String("dirty".into())]),
		);
		let sub = FlowgraphChannelSubscribe::from_meta("n", &meta_with(p)).unwrap();
		let missing_required = ChannelDatum::new("x".into(), "".into());
		let has_required = ChannelDatum::new("x".into(), "".into()).with_flag("needed");
		let has_required_but_dropped = ChannelDatum::new("x".into(), "".into())
			.with_flag("needed")
			.with_flag("dirty");
		assert!(!sub.matches(&missing_required));
		assert!(sub.matches(&has_required));
		assert!(!sub.matches(&has_required_but_dropped));
	}

	#[test]
	fn build_trigger_fills_all_overrides() {
		let sub = FlowgraphChannelSubscribe::from_meta("my_node", &meta_with(InputMap::new())).unwrap();
		let cd = ChannelDatum::new("user".into(), "hello".into())
			.with_flag(ChannelDatum::FLAG_IS_FINAL)
			.with_meta("source_actor", "alice");
		let ev = sub.build_trigger(&cd);
		assert_eq!(ev.node_id, "my_node");
		assert!(ev.fired_exec.contains(&"__trigger__".to_string()));
		let content = ev.data_overrides.get("__content__").unwrap();
		assert_eq!(content.as_str().unwrap(), "hello");
		let channel = ev.data_overrides.get("__channel__").unwrap();
		assert_eq!(channel.as_str().unwrap(), "user");
		let actor = ev.data_overrides.get("__source_actor__").unwrap();
		assert_eq!(actor.as_str().unwrap(), "alice");
		let is_final = ev.data_overrides.get("__is_final__").unwrap();
		assert!(is_final.as_bool().unwrap());
	}
}
