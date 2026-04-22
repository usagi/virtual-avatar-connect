//! δ-9 Part B: Flowgraph ランタイムの外部ブリッジ層。
//!
//! ## 責務
//!
//! - `flowgraph.ingress.*` ノードの `properties` を走査し、実行中の program に
//!   `TriggerEvent` を送り込む「入口」を提供する。
//! - 対応 ingress:
//!   - [`web_input`]: HTTP POST/GET を actix-web にぶら下げる。
//!   - [`voice`] (δ-9 Part E.2): Vosk / Whisper を起動し、確定テキストを TriggerEvent で流す。
//!     実装は V1 と共用の `processor::voice::VoiceSink` 抽象を経由する。
//!   - [`twitch`] (stub): IRC / EventSub 起動（実装は V1 processor 側を再利用する予定）。
//!   - [`channel_subscribe`] (δ-9 Part E): `State.channel_datum_tx` broadcast を subscribe し、
//!     フィルタ通過 datum を `TriggerEvent` として投入する。`channel.emit` の対称入口。
//!
//! ## 典型的な使い方
//!
//! 1. [`collect_all`] で `FlowgraphRuntime::node_meta` から ingress エントリ一覧を作る。
//! 2. actix-web の `configure` コールバック内で [`web_input::register_routes`] を呼ぶ。
//! 3. voice / twitch ingress は起動時に個別に spawn する（今後実装）。
//!
//! 全ブリッジは `TriggerHandle` を clone して所有するだけなので、program ワーカーとは
//! 非同期疎結合。program 停止時に `TriggerHandle.send` が失敗したら warn ログを出して落ちる。

pub mod channel_subscribe;
pub mod twitch;
pub mod twitch_eventsub;
pub mod voice;
pub mod web_input;

use crate::flowgraph::loader::LoadedNodeMeta;
use std::collections::HashMap;

/// ingress ノード種別ごとに整形した「接続先メタデータ」の束。
#[derive(Debug, Default, Clone)]
pub struct BridgeCatalog {
	pub web_input: Vec<web_input::FlowgraphWebInputEndpoint>,
	pub voice: Vec<voice::FlowgraphVoiceIngress>,
	pub twitch: Vec<twitch::FlowgraphTwitchIngress>,
	pub twitch_eventsub: Vec<twitch_eventsub::FlowgraphTwitchEventsubIngress>,
	pub channel_subscribe: Vec<channel_subscribe::FlowgraphChannelSubscribe>,
}

impl BridgeCatalog {
	pub fn is_empty(&self) -> bool {
		self.web_input.is_empty()
			&& self.voice.is_empty()
			&& self.twitch.is_empty()
			&& self.twitch_eventsub.is_empty()
			&& self.channel_subscribe.is_empty()
	}

	pub fn len(&self) -> usize {
		self.web_input.len()
			+ self.voice.len()
			+ self.twitch.len()
			+ self.twitch_eventsub.len()
			+ self.channel_subscribe.len()
	}
}

/// `FlowgraphRuntime::node_meta` を走査して ingress エントリを全部集める。
pub fn collect_all(node_meta: &HashMap<String, LoadedNodeMeta>) -> BridgeCatalog {
	let mut cat = BridgeCatalog::default();
	for (fq, meta) in node_meta {
		match meta.feature.as_str() {
			"flowgraph.ingress.web_input" => {
				if let Some(ep) = web_input::FlowgraphWebInputEndpoint::from_meta(fq, meta) {
					cat.web_input.push(ep);
				}
			}
			"flowgraph.ingress.voice" => {
				if let Some(v) = voice::FlowgraphVoiceIngress::from_meta(fq, meta) {
					cat.voice.push(v);
				}
			}
			"flowgraph.ingress.twitch" => {
				if let Some(t) = twitch::FlowgraphTwitchIngress::from_meta(fq, meta) {
					cat.twitch.push(t);
				}
			}
			"flowgraph.ingress.twitch_eventsub" => {
				if let Some(t) = twitch_eventsub::FlowgraphTwitchEventsubIngress::from_meta(fq, meta) {
					cat.twitch_eventsub.push(t);
				}
			}
			"flowgraph.ingress.channel_subscribe" => {
				if let Some(s) = channel_subscribe::FlowgraphChannelSubscribe::from_meta(fq, meta) {
					cat.channel_subscribe.push(s);
				}
			}
			_ => {}
		}
	}
	cat
}
