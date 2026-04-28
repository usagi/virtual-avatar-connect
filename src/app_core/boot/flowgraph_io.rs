use crate::conf::Conf;
use crate::state::{ChannelDatum, SharedState};
use crate::{bridges, flowgraph, processor, web_interface, Result};
use std::sync::Arc;
use tokio::sync::broadcast;

pub(super) struct FlowgraphIo {
	pub(super) ingress_handles: processor::ingress::IngressHandles,
	pub(super) web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
	pub(super) web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
	pub(super) trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
}

struct FlowgraphBridgeSnapshot {
	catalog: bridges::BridgeCatalog,
	trigger: Option<flowgraph::node::TriggerHandle>,
	channel_datum_tx: broadcast::Sender<ChannelDatum>,
}

async fn collect_flowgraph_bridge_snapshot(state: &SharedState) -> FlowgraphBridgeSnapshot {
	{
		let s = state.read().await;
		let fg = s.flowgraph.read().await;
		let tx = s.channel_datum_tx.clone();
		if let Some(rt) = fg.as_ref() {
			FlowgraphBridgeSnapshot {
				catalog: bridges::collect_all(&rt.node_meta),
				trigger: rt.trigger(),
				channel_datum_tx: tx,
			}
		} else {
			FlowgraphBridgeSnapshot {
				catalog: bridges::BridgeCatalog::default(),
				trigger: None,
				channel_datum_tx: tx,
			}
		}
	}
}

pub(super) async fn prepare_flowgraph_io(conf: &Conf, state: &SharedState) -> Result<FlowgraphIo> {
	let snapshot = collect_flowgraph_bridge_snapshot(state).await;
	let v2_eventsub_skip_broadcasters = {
		let username_fallback = conf.twitch.as_ref().map(|t| t.username.clone()).unwrap_or_default();
		bridges::twitch_eventsub::v1_skip_broadcaster_logins(&snapshot.catalog.twitch_eventsub, &username_fallback)
	};

	let (ingress_handles, web_input_registry) = processor::ingress::prepare(conf, state.clone(), &v2_eventsub_skip_broadcasters).await?;

	let initial_bridges = bridges::spawn_all_from_state(state, &snapshot.channel_datum_tx).await;
	let web_input_endpoints = Arc::new(initial_bridges.web_input_snapshot.clone());
	{
		let s = state.read().await;
		let mut slot = s.bridge_handles.lock().await;
		*slot = initial_bridges;
	}

	Ok(FlowgraphIo {
		ingress_handles,
		web_input_registry,
		web_input_endpoints,
		trigger: Arc::new(snapshot.trigger),
	})
}
