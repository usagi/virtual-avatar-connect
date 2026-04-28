//! VMC 互換の **生 UDP** 受信とパススルー転送。

use crate::conf::VmcPassthroughSpec;
use crate::motion::status::VmcPassthroughStatusEntry;
use crate::shutdown::ShutdownBroker;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use vac_motion::router::{self, SendFailLogThrottle};

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
	status: Option<Arc<VmcPassthroughStatusEntry>>,
) -> JoinHandle<()> {
	let send_fail = SendFailLogThrottle::new();
	tokio::spawn(async move {
		let sock = match UdpSocket::bind(bind).await {
			Ok(s) => s,
			Err(e) => {
				log::error!("《Motion/VMC》[{}] UDP bind 失敗 {}: {}", log_ctx, bind, e);
				if let Some(status) = status.as_ref() {
					status.mark_failed(format!("UDP bind 失敗 {bind}: {e}"));
				}
				return;
			}
		};
		if let Some(status) = status.as_ref() {
			status.mark_running();
		}
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
				let current_forward = status
					.as_ref()
					.map(|status| status.forward_addrs_snapshot())
					.unwrap_or_else(|| forward.clone());
				let send_errors = router::forward_datagram(&sock, payload, &current_forward, &log_ctx, &send_fail).await;
				if let Some(status) = status.as_ref() {
					status.record_receive(n, current_forward.len().saturating_sub(send_errors));
					for _ in 0..send_errors {
						status.record_send_error();
					}
				}
				log::trace!(
					"《Motion/VMC》[{}] {} bytes from {} → {} 先",
					log_ctx,
					n,
					src,
					current_forward.len()
				);
			   }
			   Err(e) => {
				log::warn!("《Motion/VMC》[{}] recv_from: {}", log_ctx, e);
			   }
			  }
			 }
			}
		}
		if let Some(status) = status.as_ref() {
			status.mark_stopped();
		}
	})
}

/// 設定を検証し、有効ならタスクを返す。
pub fn try_spawn(
	spec: &VmcPassthroughSpec,
	shutdown: Arc<ShutdownBroker>,
	status: Option<Arc<VmcPassthroughStatusEntry>>,
) -> Option<JoinHandle<()>> {
	if !spec.enabled {
		if let Some(status) = status.as_ref() {
			status.mark_skipped("disabled");
		}
		return None;
	}
	if spec.forward_to.is_empty() {
		log::warn!(
			"《Motion/VMC》 vmc_passthrough bind={:?} は forward_to が空のためスキップします",
			spec.bind
		);
		if let Some(status) = status.as_ref() {
			status.mark_skipped("forward_to が空");
		}
		return None;
	}
	let bind = match parse_bind(spec) {
		Ok(a) => a,
		Err(e) => {
			log::error!("《Motion/VMC》 {}", e);
			if let Some(status) = status.as_ref() {
				status.mark_failed(e);
			}
			return None;
		}
	};
	let forward = match parse_forwards(spec) {
		Ok(v) => v,
		Err(e) => {
			log::error!("《Motion/VMC》 {}", e);
			if let Some(status) = status.as_ref() {
				status.mark_failed(e);
			}
			return None;
		}
	};
	let log_ctx = passthrough_log_ctx(spec, bind);
	Some(spawn_passthrough_task(bind, forward, shutdown, log_ctx, status))
}
