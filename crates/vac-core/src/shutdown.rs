//! Phase ε-1: シャットダウン統合ブローカー。
//!
//! VAC の停止経路は従来、以下の 3 つが独立に存在していて相互連携がなかった:
//!
//!   1. `Ctrl+C` → `tokio::signal::ctrl_c()`（LibreTranslate を止めるだけ）
//!   2. actix-web 内部の SIGINT ハンドラ（HTTP サーバーを止めるだけ）
//!   3. Control API からの再起動（`process::exit(0)` 直叩き）
//!
//! 結果として:
//!   - 1 回目の Ctrl+C で本当に停止したいリソース（ManagedApp = run_with 子プロセス、
//!     voice ワーカーなど）が落ちないケースがある
//!   - ユーザーが 2 回目の Ctrl+C を押すと Windows 側で強制終了され、
//!     終了コードが `STATUS_CONTROL_C_EXIT (0xc000013a)` になる
//!   - GUI からの「穏やかな終了」手段が存在しない
//!
//! これを解消するため、**`ShutdownBroker`** を 1 本立てて:
//!
//!   - Ctrl+C / `POST /api/v1/control/shutdown` / 致命的エラーはすべて `trigger(reason)` に集約
//!   - 各サブシステム（actix / ManagedApp monitor / bridges など）は `wait()` で待機
//!   - actix-web 側は `disable_signals()` + 自前の `ServerHandle::stop(true)` に切り替えて
//!     「broker 経由の停止」だけを唯一の停止経路にする
//!
//! という構造に揃える。将来 Tauri window の `CloseRequested` からも同じ broker を叩くだけで
//! 統合できる、というのが副次的な狙い（[docs/roadmap/phase-epsilon-shutdown-and-tauri.md]）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// どこから停止要求が来たか。主にログ出力用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownReason {
	/// ターミナルで Ctrl+C が押された。
	CtrlC,
	/// `POST /api/v1/control/shutdown`（GUI の「アプリ終了」ボタン含む）。
	ControlApi,
	/// desktop runner の tray / window close からの停止要求。
	Desktop,
	/// 将来的な致命的エラー経路（現状は未使用だが、今後 `run()` 内で `?` を直叩きする前に
	/// 明示的に `trigger(Fatal)` を呼べるように取っておく）。
	Fatal,
}

impl ShutdownReason {
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::CtrlC => "ctrl_c",
			Self::ControlApi => "control_api",
			Self::Desktop => "desktop",
			Self::Fatal => "fatal",
		}
	}
}

/// 停止フラグと `tokio::sync::Notify` を束ねた一元ブローカー。
///
/// - `trigger()` は何回呼ばれても安全で、最初の呼び出しだけ reason を記録し waiters を起こす。
/// - `wait()` は既に `triggered` 済みなら即 `ready`、そうでなければ `notified()` 待機。
/// - 参照は `Arc<ShutdownBroker>` で共有する（`State.shutdown` に持たせる）。
#[derive(Debug)]
pub struct ShutdownBroker {
	notify: Notify,
	triggered: AtomicBool,
	reason: Mutex<Option<ShutdownReason>>,
}

impl ShutdownBroker {
	pub fn new() -> Arc<Self> {
		Arc::new(Self {
			notify: Notify::new(),
			triggered: AtomicBool::new(false),
			reason: Mutex::new(None),
		})
	}

	/// 停止要求を発する。2 回目以降の呼び出しは trace に落として無視する（冪等）。
	pub fn trigger(&self, reason: ShutdownReason) {
		let was = self.triggered.swap(true, Ordering::SeqCst);
		if !was {
			if let Ok(mut g) = self.reason.lock() {
				*g = Some(reason);
			}
			log::info!("《Shutdown》 トリガ受信: reason={}", reason.as_str());
			self.notify.notify_waiters();
		} else {
			log::trace!("《Shutdown》 二重トリガ（無視）: reason={}（既に triggered）", reason.as_str());
		}
	}

	pub fn is_triggered(&self) -> bool {
		self.triggered.load(Ordering::SeqCst)
	}

	pub fn reason(&self) -> Option<ShutdownReason> {
		self.reason.lock().ok().and_then(|g| *g)
	}

	/// 停止が来るまで待つ。複数タスクから同時に呼ばれても全員が起きる（`notify_waiters` 経由）。
	///
	/// 典型用途:
	///   - actix の `ServerHandle::stop(true)` を叩くための専用 listener タスク
	///   - `managed_app::run_monitor` の内側のループで `tokio::select!` と組み合わせて早期 break
	pub async fn wait(&self) {
		// Notify の race を避けるため、permit を先に確保してから triggered を再確認する。
		let notified = self.notify.notified();
		tokio::pin!(notified);
		if self.is_triggered() {
			return;
		}
		notified.await;
	}
}

/// `tokio::signal::ctrl_c()` を待つリスナータスクを spawn する。
///
/// **設計**:
///   - 1 回目: broker に `CtrlC` を trigger（= graceful shutdown を開始）
///   - 2 回目: まだ shutdown 中なので warn ログを出してユーザーに「処理中」と伝える
///   - 3 回目: ユーザーが明確に「もう待てない」と言っているので `std::process::exit(130)` で
///     即座にプロセスを落とす（130 = 128 + SIGINT）。ManagedApp の子は OS に任せる。
///
/// **なぜ loop が必要か**: `tokio::signal::ctrl_c()` の await は 1 回だけイベントを消費する。
/// そのあと task が終了すると次の Ctrl+C イベントは購読者が居ない状態になり、Windows で
/// cargo が子を force-kill して `STATUS_CONTROL_C_EXIT` に見えるケースがあった。
/// ループで消費し続けることで、複数回の Ctrl+C を全て自前ハンドラで受けきる。
pub fn spawn_ctrl_c_listener(broker: Arc<ShutdownBroker>) {
	tokio::spawn(async move {
		let mut count: u32 = 0;
		loop {
			match tokio::signal::ctrl_c().await {
				Ok(()) => {
					count = count.saturating_add(1);
					match count {
						1 => {
							broker.trigger(ShutdownReason::CtrlC);
						}
						2 => {
							log::warn!("《Shutdown》 Ctrl+C を 2 回受信しました。既に停止処理中です。もう 1 回押すと強制終了します。");
						}
						_ => {
							log::error!(
        "《Shutdown》 Ctrl+C を 3 回以上受信しました。強制終了します（ManagedApp 子プロセスの後始末は OS に委ねます）。"
       );
							std::process::exit(130);
						}
					}
				}
				Err(e) => {
					log::error!("《Shutdown》 tokio::signal::ctrl_c() の待機に失敗: {e}");
					break;
				}
			}
		}
	});
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	#[tokio::test]
	async fn trigger_then_wait_returns_immediately() {
		let b = ShutdownBroker::new();
		b.trigger(ShutdownReason::CtrlC);
		assert!(b.is_triggered());
		// 既に triggered 済みなら await は即時完了する
		tokio::time::timeout(Duration::from_millis(50), b.wait())
			.await
			.expect("wait should return immediately after trigger");
		assert_eq!(b.reason(), Some(ShutdownReason::CtrlC));
	}

	#[tokio::test]
	async fn wait_then_trigger_wakes_waiter() {
		let b = ShutdownBroker::new();
		let b2 = b.clone();
		let handle = tokio::spawn(async move {
			b2.wait().await;
		});
		tokio::time::sleep(Duration::from_millis(10)).await;
		b.trigger(ShutdownReason::ControlApi);
		tokio::time::timeout(Duration::from_millis(100), handle)
			.await
			.expect("waiter should wake after trigger")
			.expect("task should not panic");
		assert_eq!(b.reason(), Some(ShutdownReason::ControlApi));
	}

	#[tokio::test]
	async fn trigger_is_idempotent() {
		let b = ShutdownBroker::new();
		b.trigger(ShutdownReason::ControlApi);
		b.trigger(ShutdownReason::CtrlC);
		// 最初の reason が記録され続ける
		assert_eq!(b.reason(), Some(ShutdownReason::ControlApi));
	}
}
