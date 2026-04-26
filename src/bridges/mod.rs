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
//!   - `vmc_ingress` (Phase M1): `flowgraph.ingress.vmc_udp` — 生 UDP を受信し Base64 化して `TriggerEvent` 投入。
//!
//! ## 典型的な使い方
//!
//! 1. [`collect_all`] で `FlowgraphRuntime::node_meta` から ingress エントリ一覧を作る。
//! 2. actix-web の `configure` コールバック内で [`web_input::register_routes`] を呼ぶ。
//! 3. voice / twitch ingress は起動時に個別に spawn する（今後実装）。
//!
//! 全ブリッジは `TriggerHandle` を clone して所有するだけなので、program ワーカーとは
//! 非同期疎結合。program 停止時に `TriggerHandle.send` が失敗したら warn ログを出して落ちる。
//!
//! ## Crate 分割時の依存契約（Step 4）
//!
//! - **許容**: `flowgraph`（`loader` / `node` / `socket` 等）、`state` の共有型、`shutdown`（ingress の寿命）、`processor`（voice）。
//! - **禁止**: `motion` への参照（UDP パススルーは motion 層が単独で完結）。`web_interface` へも直接依存しない。
//! 詳細表は [`docs/architecture.md`](../../docs/architecture.md)「レイヤ境界（Step 4）」。

pub mod channel_subscribe;
pub mod twitch;
pub mod twitch_eventsub;
pub mod vmc_ingress;
pub mod voice;
pub mod web_input;

use crate::flowgraph::loader::LoadedNodeMeta;
use crate::state::ChannelDatum;
use crate::SharedState;
use std::collections::HashMap;
use tokio::sync::broadcast;

/// ingress ノード種別ごとに整形した「接続先メタデータ」の束。
#[derive(Debug, Default, Clone)]
pub struct BridgeCatalog {
	pub web_input: Vec<web_input::FlowgraphWebInputEndpoint>,
	pub voice: Vec<voice::FlowgraphVoiceIngress>,
	pub twitch: Vec<twitch::FlowgraphTwitchIngress>,
	pub twitch_eventsub: Vec<twitch_eventsub::FlowgraphTwitchEventsubIngress>,
	pub channel_subscribe: Vec<channel_subscribe::FlowgraphChannelSubscribe>,
	pub vmc_udp: Vec<vmc_ingress::FlowgraphVmcUdpIngress>,
}

impl BridgeCatalog {
	pub fn is_empty(&self) -> bool {
		self.web_input.is_empty()
			&& self.voice.is_empty()
			&& self.twitch.is_empty()
			&& self.twitch_eventsub.is_empty()
			&& self.channel_subscribe.is_empty()
			&& self.vmc_udp.is_empty()
	}

	pub fn len(&self) -> usize {
		self.web_input.len()
			+ self.voice.len()
			+ self.twitch.len()
			+ self.twitch_eventsub.len()
			+ self.channel_subscribe.len()
			+ self.vmc_udp.len()
	}
}

/// ζ-3: Flowgraph 実行中に走っている bridge ワーカーたちをまとめた lifecycle 管理用ハンドル。
///
/// - 初回起動時: `lib.rs::run` が [`spawn_all_from_state`] を呼び、戻りを `State` に格納する。
/// - reload 時: [`web_interface::control::flowgraph::reload_runtime`] が旧 handles を取り出して
///   [`BridgeHandles::finish_all`] で停止 → 新 runtime 上で再 spawn → 入れ替え、という流れ。
///
/// `web_input_snapshot` は actix HTTP server を再起動できない制約上、差分検出用にだけ持つ。
/// 差分があれば GUI 側に [`crate::web_interface::control::events::ControlEvent::RestartRecommended`]
/// をブロードキャストして再起動を促す。
pub struct BridgeHandles {
	pub(crate) twitch: Vec<twitch::TwitchBridgeHandle>,
	pub(crate) twitch_eventsub: Vec<twitch_eventsub::TwitchEventsubBridgeHandle>,
	pub(crate) voice: Vec<crate::processor::voice::VoiceIngress>,
	pub(crate) channel_subscribe: Vec<tokio::task::JoinHandle<()>>,
	pub(crate) vmc_udp: Vec<tokio::task::JoinHandle<()>>,
	/// actix に登録済みの web_input エンドポイントのスナップショット。reload 差分検出専用。
	pub(crate) web_input_snapshot: Vec<web_input::FlowgraphWebInputEndpoint>,
}

impl Default for BridgeHandles {
	fn default() -> Self {
		Self::empty()
	}
}

impl std::fmt::Debug for BridgeHandles {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("BridgeHandles")
			.field("twitch", &self.twitch.len())
			.field("twitch_eventsub", &self.twitch_eventsub.len())
			.field("voice", &self.voice.len())
			.field("channel_subscribe", &self.channel_subscribe.len())
			.field("vmc_udp", &self.vmc_udp.len())
			.field("web_input_snapshot", &self.web_input_snapshot.len())
			.finish()
	}
}

impl BridgeHandles {
	pub fn empty() -> Self {
		Self {
			twitch: Vec::new(),
			twitch_eventsub: Vec::new(),
			voice: Vec::new(),
			channel_subscribe: Vec::new(),
			vmc_udp: Vec::new(),
			web_input_snapshot: Vec::new(),
		}
	}

	/// 所有しているハンドルを全て graceful shutdown する。個々の finish は 5 秒タイムアウト。
	pub async fn finish_all(self) {
		for h in self.twitch_eventsub {
			let node_id = h.node_id().to_string();
			if tokio::time::timeout(std::time::Duration::from_secs(5), h.finish()).await.is_err() {
				log::warn!("《Bridges》 twitch_eventsub finish タイムアウト node={}", node_id);
			}
		}
		for h in self.twitch {
			if tokio::time::timeout(std::time::Duration::from_secs(5), h.finish()).await.is_err() {
				log::warn!("《Bridges》 twitch IRC finish タイムアウト");
			}
		}
		for h in self.voice {
			if tokio::time::timeout(std::time::Duration::from_secs(5), h.finish()).await.is_err() {
				log::warn!("《Bridges》 voice finish タイムアウト");
			}
		}
		// channel_subscribe は JoinHandle を抱えるだけ。State.channel_datum_tx の subscribe は
		// 自身が abort されない限り生きつづけるので、明示 abort する。
		for h in self.channel_subscribe {
			h.abort();
		}
		for h in self.vmc_udp {
			h.abort();
		}
	}
}

/// ζ-3: `SharedState` 配下の flowgraph runtime を読み、全 bridge を新しい trigger で spawn する。
///
/// 戻りには web_input のスナップショットも入るが、actix HTTP server 側の route は
/// 起動時に固定されるため、新しいエンドポイントが追加された場合は呼び出し側で差分を見て
/// `RestartRecommended` 通知を出すこと（ここでは投げない）。
pub async fn spawn_all_from_state(state: &SharedState, channel_datum_tx: &broadcast::Sender<ChannelDatum>) -> BridgeHandles {
	let (catalog, trigger) = {
		let s = state.read().await;
		let fg = s.flowgraph.read().await;
		match fg.as_ref() {
			Some(rt) => (collect_all(&rt.node_meta), rt.trigger()),
			None => (BridgeCatalog::default(), None),
		}
	};

	if !catalog.is_empty() {
		log::info!(
			"《Flowgraph/Bridges》 ingress 合計 {} 件: web_input={}, voice={}, twitch={}, twitch_eventsub={}, channel_subscribe={}, vmc_udp={}",
			catalog.len(),
			catalog.web_input.len(),
			catalog.voice.len(),
			catalog.twitch.len(),
			catalog.twitch_eventsub.len(),
			catalog.channel_subscribe.len(),
			catalog.vmc_udp.len()
		);
	}

	let shutdown = {
		let s = state.read().await;
		s.shutdown.clone()
	};

	let tokio_handle = tokio::runtime::Handle::current();
	let voice = voice::spawn(&catalog.voice, trigger.clone(), tokio_handle);
	let twitch = twitch::spawn(&catalog.twitch, trigger.clone(), state.clone()).await;
	let twitch_eventsub = twitch_eventsub::spawn(&catalog.twitch_eventsub, trigger.clone(), state.clone());
	let channel_subscribe = channel_subscribe::spawn(&catalog.channel_subscribe, trigger.clone(), channel_datum_tx);
	let vmc_udp = vmc_ingress::spawn(&catalog.vmc_udp, trigger.clone(), shutdown);

	BridgeHandles {
		twitch,
		twitch_eventsub,
		voice,
		channel_subscribe,
		vmc_udp,
		web_input_snapshot: catalog.web_input,
	}
}

/// ζ-3: `web_input_snapshot` 間の差分（method + path の集合）が実質的に変化したかを返す。
///
/// actix-web は route 登録を再起動無しに差し替えられないため、reload 時に本判定で変化を検出したら
/// `RestartRecommended` 通知を出して再起動を促す。
pub fn web_input_changed(old: &[web_input::FlowgraphWebInputEndpoint], new: &[web_input::FlowgraphWebInputEndpoint]) -> bool {
	fn key(ep: &web_input::FlowgraphWebInputEndpoint) -> (String, web_input::Method) {
		(ep.path.clone(), ep.method)
	}
	let mut old_set: std::collections::BTreeSet<_> = old.iter().map(key).collect();
	let mut new_set: std::collections::BTreeSet<_> = new.iter().map(key).collect();
	// 対称差が空ならば変化なし。
	old_set.retain(|k| !new_set.remove(k));
	!(old_set.is_empty() && new_set.is_empty())
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
			"flowgraph.ingress.vmc_udp" => {
				if let Some(v) = vmc_ingress::FlowgraphVmcUdpIngress::from_meta(fq, meta) {
					cat.vmc_udp.push(v);
				}
			}
			_ => {}
		}
	}
	cat
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bridges::web_input::{BodyFormat, FlowgraphWebInputEndpoint, Method};

	fn ep(path: &str, method: Method) -> FlowgraphWebInputEndpoint {
		FlowgraphWebInputEndpoint {
			node_id: format!("n{}", path),
			path: path.to_string(),
			method,
			body_format: BodyFormat::Plain,
			fixed_channel: String::new(),
		}
	}

	#[test]
	fn web_input_changed_detects_add_remove_and_ignores_order() {
		let a = vec![ep("/x", Method::Post), ep("/y", Method::Get)];
		let b_reorder = vec![ep("/y", Method::Get), ep("/x", Method::Post)];
		assert!(!web_input_changed(&a, &b_reorder), "順序入れ替えは同一扱い");

		let b_add = vec![ep("/x", Method::Post), ep("/y", Method::Get), ep("/z", Method::Put)];
		assert!(web_input_changed(&a, &b_add), "追加は差分");

		let b_rm = vec![ep("/x", Method::Post)];
		assert!(web_input_changed(&a, &b_rm), "削除は差分");

		let b_method = vec![ep("/x", Method::Get), ep("/y", Method::Get)];
		assert!(web_input_changed(&a, &b_method), "method 違いは差分");

		assert!(!web_input_changed(&a, &a), "恒等は同一扱い");
		assert!(!web_input_changed(&[], &[]), "両空は同一扱い");
	}

	#[test]
	fn bridge_handles_default_is_empty() {
		let h = BridgeHandles::default();
		assert!(h.twitch.is_empty());
		assert!(h.twitch_eventsub.is_empty());
		assert!(h.voice.is_empty());
		assert!(h.channel_subscribe.is_empty());
		assert!(h.vmc_udp.is_empty());
		assert!(h.web_input_snapshot.is_empty());
	}

	#[test]
	fn bridge_handles_debug_shows_counts() {
		let h = BridgeHandles::empty();
		let s = format!("{h:?}");
		assert!(s.contains("BridgeHandles"));
		assert!(s.contains("twitch"));
	}
}
