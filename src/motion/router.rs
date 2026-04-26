//! 生 UDP ペイロードの **パースなし** マルチキャスト転送。

use std::net::SocketAddr;
use tokio::net::UdpSocket;

/// `payload` を各 `dests` へ同一内容で `send_to` する。失敗は `warn!` のみ（ホットパスを止めない）。
pub async fn forward_datagram(sock: &UdpSocket, payload: &[u8], dests: &[SocketAddr]) {
 for d in dests {
  if let Err(e) = sock.send_to(payload, *d).await {
   log::warn!("《Motion/VMC》 send_to {} 失敗: {}", d, e);
  }
 }
}
