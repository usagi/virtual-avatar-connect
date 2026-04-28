//! 生 UDP ペイロードの **パースなし** マルチキャスト転送。

use std::io;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

const SEND_FAIL_LOG_MIN_INTERVAL: Duration = Duration::from_secs(5);

/// `send_to` 失敗ログを間引く（宛先ダウン時のログ洪水を防ぐ）。タスク 1 本あたり 1 インスタンスを共有する。
pub struct SendFailLogThrottle {
	inner: Mutex<(Instant, u32)>,
}

impl SendFailLogThrottle {
	pub fn new() -> Self {
		Self {
			inner: Mutex::new((Instant::now() - SEND_FAIL_LOG_MIN_INTERVAL, 0)),
		}
	}

	pub fn log_send_to_failure(&self, ctx: &str, dest: SocketAddr, err: &io::Error) {
		let mut g = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		let now = Instant::now();
		if now.duration_since(g.0) >= SEND_FAIL_LOG_MIN_INTERVAL {
			if g.1 > 0 {
				log::warn!(
					"《Motion/VMC》[{}] send_to 失敗を直近 {} 件省略（まとめログは最大 {} 秒に 1 回）",
					ctx,
					g.1,
					SEND_FAIL_LOG_MIN_INTERVAL.as_secs()
				);
			}
			log::warn!("《Motion/VMC》[{}] send_to {} 失敗: {}", ctx, dest, err);
			g.0 = now;
			g.1 = 0;
		} else {
			g.1 += 1;
		}
	}
}

/// `payload` を各 `dests` へ同一内容で `send_to` する。失敗は `throttle` 経由で `warn!`（ホットパスを止めない）。
///
/// 戻り値は失敗した送信数。Control API の統計表示用で、転送ループ自体は止めない。
pub async fn forward_datagram(sock: &UdpSocket, payload: &[u8], dests: &[SocketAddr], ctx: &str, send_fail: &SendFailLogThrottle) -> usize {
	let mut errors = 0;
	for d in dests {
		if let Err(e) = sock.send_to(payload, *d).await {
			send_fail.log_send_to_failure(ctx, *d, &e);
			errors += 1;
		}
	}
	errors
}
