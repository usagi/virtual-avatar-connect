//! VMC 互換の **生 UDP** 受信とパススルー転送。

use crate::conf::VmcPassthroughSpec;
use crate::motion::router::{self, SendFailLogThrottle};
use crate::shutdown::ShutdownBroker;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

/// ログ行に載せる短い文脈（`label` が空なら `bind`）。
pub(crate) fn passthrough_log_ctx(spec: &VmcPassthroughSpec, bind: SocketAddr) -> String {
	spec.label
		.as_deref()
		.map(str::trim)
		.filter(|s| !s.is_empty())
		.map(str::to_owned)
		.unwrap_or_else(|| bind.to_string())
}

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
pub fn spawn_passthrough_task(
	bind: SocketAddr,
	forward: Vec<SocketAddr>,
	shutdown: Arc<ShutdownBroker>,
	log_ctx: String,
) -> JoinHandle<()> {
	let send_fail = SendFailLogThrottle::new();
	tokio::spawn(async move {
		let sock = match UdpSocket::bind(bind).await {
			Ok(s) => s,
			Err(e) => {
				log::error!("《Motion/VMC》[{}] UDP bind 失敗 {}: {}", log_ctx, bind, e);
				return;
			}
		};
		let dests = forward.iter().map(SocketAddr::to_string).collect::<Vec<_>>().join(", ");
		log::info!(
			"《Motion/VMC》[{}] passthrough 起動 bind={} → [{}]（{} 宛先）",
			log_ctx,
			bind,
			dests,
			forward.len()
		);
		let mut buf = vec![0u8; 65535];
		loop {
			tokio::select! {
			 biased;
			 _ = shutdown.wait() => {
			  log::info!("《Motion/VMC》[{}] passthrough 終了 bind={}", log_ctx, bind);
			  break;
			 }
			 res = sock.recv_from(&mut buf) => {
			  match res {
			   Ok((n, src)) => {
				if n == 0 {
				 continue;
				}
				let payload = &buf[..n];
				router::forward_datagram(&sock, payload, &forward, &log_ctx, &send_fail).await;
				log::trace!(
					"《Motion/VMC》[{}] {} bytes from {} → {} 先",
					log_ctx,
					n,
					src,
					forward.len()
				);
			   }
			   Err(e) => {
				log::warn!("《Motion/VMC》[{}] recv_from: {}", log_ctx, e);
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
	let log_ctx = passthrough_log_ctx(spec, bind);
	Some(spawn_passthrough_task(bind, forward, shutdown, log_ctx))
}
