//! VMC 互換の **生 UDP** 受信とパススルー転送。

use crate::conf::VmcPassthroughSpec;
use crate::motion::router;
use crate::shutdown::ShutdownBroker;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

fn parse_bind(spec: &VmcPassthroughSpec) -> Result<SocketAddr, String> {
	spec.bind
		.trim()
		.parse::<SocketAddr>()
		.map_err(|e| format!("bind {:?}: {}", spec.bind, e))
}

fn parse_forwards(spec: &VmcPassthroughSpec) -> Result<Vec<SocketAddr>, String> {
	let mut out = Vec::with_capacity(spec.forward_to.len());
	for (i, s) in spec.forward_to.iter().enumerate() {
		let a = s
			.trim()
			.parse::<SocketAddr>()
			.map_err(|e| format!("forward_to[{}] {:?}: {}", i, s, e))?;
		out.push(a);
	}
	Ok(out)
}

/// 1 本の passthrough ワーカーを spawn する。
pub fn spawn_passthrough_task(bind: SocketAddr, forward: Vec<SocketAddr>, shutdown: Arc<ShutdownBroker>) -> JoinHandle<()> {
	tokio::spawn(async move {
		let sock = match UdpSocket::bind(bind).await {
			Ok(s) => s,
			Err(e) => {
				log::error!("《Motion/VMC》 UDP bind 失敗 {}: {}", bind, e);
				return;
			}
		};
		log::info!("《Motion/VMC》 passthrough 起動 bind={} → {} 宛先", bind, forward.len());
		let mut buf = vec![0u8; 65535];
		loop {
			tokio::select! {
			 biased;
			 _ = shutdown.wait() => {
			  log::info!("《Motion/VMC》 passthrough 終了 bind={}", bind);
			  break;
			 }
			 res = sock.recv_from(&mut buf) => {
			  match res {
			   Ok((n, src)) => {
				if n == 0 {
				 continue;
				}
				let payload = &buf[..n];
				router::forward_datagram(&sock, payload, &forward).await;
				log::trace!("《Motion/VMC》 {} bytes from {} → {} 先", n, src, forward.len());
			   }
			   Err(e) => {
				log::warn!("《Motion/VMC》 recv_from: {}", e);
			   }
			  }
			 }
			}
		}
	})
}

/// 設定を検証し、有効ならタスクを返す。
pub fn try_spawn(spec: &VmcPassthroughSpec, shutdown: Arc<ShutdownBroker>) -> Option<JoinHandle<()>> {
	if !spec.enabled {
		return None;
	}
	if spec.forward_to.is_empty() {
		log::warn!(
			"《Motion/VMC》 vmc_passthrough bind={:?} は forward_to が空のためスキップします",
			spec.bind
		);
		return None;
	}
	let bind = match parse_bind(spec) {
		Ok(a) => a,
		Err(e) => {
			log::error!("《Motion/VMC》 {}", e);
			return None;
		}
	};
	let forward = match parse_forwards(spec) {
		Ok(v) => v,
		Err(e) => {
			log::error!("《Motion/VMC》 {}", e);
			return None;
		}
	};
	Some(spawn_passthrough_task(bind, forward, shutdown))
}
