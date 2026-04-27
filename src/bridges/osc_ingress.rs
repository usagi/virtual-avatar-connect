//! `flowgraph.ingress.osc_udp` 用ブリッジ（汎用 OSC / 任意 UDP ペイロードの入口）。
//!
//! VMC 専用の [`super::vmc_ingress`] と **ノード・メタを分離**する（`__meta__` に `profile: "osc_udp"` を載せ、
//! `__source_kind__` 既定は `osc_udp`）。ペイロードの Base64 運搬は VMC と同型だが、意味論は汎用 OSC 向け。

use crate::flowgraph::loader::LoadedNodeMeta;
use crate::flowgraph::node::{TriggerEvent, TriggerHandle};
use crate::flowgraph::socket::SocketValue;
use crate::shutdown::ShutdownBroker;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

/// 1 つの `flowgraph.ingress.osc_udp` ノードの設定。
#[derive(Debug, Clone)]
pub struct FlowgraphOscUdpIngress {
	pub node_id: String,
	pub bind: String,
	pub fixed_channel: String,
}

impl FlowgraphOscUdpIngress {
	pub fn from_meta(fq: &str, meta: &LoadedNodeMeta) -> Option<Self> {
		let p = &meta.properties;
		let get_str = |k: &str| -> String {
			p.get(k)
				.and_then(|v| v.as_str().ok())
				.map(str::trim)
				.map(str::to_string)
				.unwrap_or_default()
		};
		let bind = get_str("bind");
		if bind.is_empty() {
			log::warn!(
				"《Flowgraph/OSC》 ingress node_id={} は property `bind` が空のためブリッジを開始しません。",
				fq
			);
			return None;
		}
		Some(Self {
			node_id: fq.to_string(),
			bind,
			fixed_channel: get_str("fixed_channel"),
		})
	}
}

fn spawn_one(entry: FlowgraphOscUdpIngress, trigger: TriggerHandle, shutdown: Arc<ShutdownBroker>) -> JoinHandle<()> {
	let node_id = entry.node_id.clone();
	let bind_str = entry.bind.clone();
	let kind = if entry.fixed_channel.trim().is_empty() {
		"osc_udp".to_string()
	} else {
		entry.fixed_channel.trim().to_string()
	};
	tokio::spawn(async move {
		let addr: SocketAddr = match bind_str.parse() {
			Ok(a) => a,
			Err(e) => {
				log::error!("《Flowgraph/OSC》 bind パース失敗 {:?}: {}", bind_str, e);
				return;
			}
		};
		let sock = match UdpSocket::bind(addr).await {
			Ok(s) => s,
			Err(e) => {
				log::error!("《Flowgraph/OSC》 UDP bind 失敗 {}: {}", addr, e);
				return;
			}
		};
		log::info!("《Flowgraph/OSC》 ingress 起動 node_id={} bind={}", node_id, addr);
		let mut buf = vec![0u8; 65535];
		loop {
			tokio::select! {
				biased;
				_ = shutdown.wait() => {
					log::info!("《Flowgraph/OSC》 ingress 終了 node_id={} bind={}", node_id, addr);
					break;
				}
				res = sock.recv_from(&mut buf) => {
					match res {
						Ok((n, src)) => {
							if n == 0 {
								continue;
							}
							let slice = &buf[..n];
							let b64 = STANDARD.encode(slice);
							let actor = format!("{}:{}", src.ip(), src.port());
							let meta = json!({
								"remote": actor.clone(),
								"byte_len": n,
								"encoding": "base64",
								"profile": "osc_udp",
							});
							let ev = TriggerEvent::new(&node_id)
								.with_exec("__trigger__")
								.with_override("__content__", SocketValue::String(b64))
								.with_override("__source_actor__", SocketValue::String(actor))
								.with_override("__source_kind__", SocketValue::String(kind.clone()))
								.with_override("__meta__", SocketValue::Json(meta));
							if let Err(e) = trigger.send(ev) {
								log::warn!("《Flowgraph/OSC》 TriggerEvent 送信失敗 node_id={}: {}", node_id, e);
							}
						}
						Err(e) => {
							log::warn!("《Flowgraph/OSC》 recv_from: {}", e);
						}
					}
				}
			}
		}
	})
}

pub fn spawn(entries: &[FlowgraphOscUdpIngress], trigger: Option<TriggerHandle>, shutdown: Arc<ShutdownBroker>) -> Vec<JoinHandle<()>> {
	if entries.is_empty() {
		return Vec::new();
	}
	let Some(trigger) = trigger else {
		log::warn!(
			"《Flowgraph/OSC》 ingress ノード {} 件を検出しましたが、Flowgraph ランタイムが無いため OSC UDP ブリッジを開始できません。",
			entries.len()
		);
		return Vec::new();
	};
	let mut out = Vec::with_capacity(entries.len());
	for e in entries {
		out.push(spawn_one(e.clone(), trigger.clone(), shutdown.clone()));
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::loader::LoadedNodeMeta;
	use crate::flowgraph::node::InputMap;
	use crate::flowgraph::socket::SocketValue;
	use std::path::PathBuf;

	fn meta(props: Vec<(&str, SocketValue)>) -> LoadedNodeMeta {
		let mut m = InputMap::new();
		for (k, v) in props {
			m.insert(k.to_string(), v);
		}
		LoadedNodeMeta {
			feature: "flowgraph.ingress.osc_udp".into(),
			file: PathBuf::from("t.flowgraph.toml"),
			position: None,
			properties: m,
		}
	}

	#[test]
	fn from_meta_requires_non_empty_bind() {
		let m = meta(vec![]);
		assert!(FlowgraphOscUdpIngress::from_meta("n1", &m).is_none());
		let m2 = meta(vec![
			("bind", SocketValue::String("127.0.0.1:0".into())),
			("fixed_channel", SocketValue::String("dmx".into())),
		]);
		let v = FlowgraphOscUdpIngress::from_meta("n1", &m2).expect("bind");
		assert_eq!(v.node_id, "n1");
		assert_eq!(v.fixed_channel, "dmx");
	}
}
