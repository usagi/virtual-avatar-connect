//! 配信者発話中などに共有する **speech floor**（キーごとに占有／待機）。
//! 《Voice》が `set_held(true)`、待機側は `wait_until_free` で `Notify` による解放を待つ（スキップしない）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Debug)]
pub struct SpeechFloorManager {
	inner: Mutex<HashMap<String, Arc<FloorState>>>,
}

#[derive(Debug)]
struct FloorState {
	held: AtomicBool,
	notify: Notify,
}

impl SpeechFloorManager {
	pub fn new() -> Self {
		Self {
			inner: Mutex::new(HashMap::new()),
		}
	}

	fn floor(&self, key: &str) -> Arc<FloorState> {
		let mut g = self.inner.lock().unwrap();
		g.entry(key.to_string())
			.or_insert_with(|| {
				Arc::new(FloorState {
					held: AtomicBool::new(false),
					notify: Notify::new(),
				})
			})
			.clone()
	}

	/// `held == true` の間、待機側は解放（`false`）まで `Notify` でブロックしない待機を繰り返す。
	pub fn set_held(&self, key: &str, held: bool) {
		let f = self.floor(key);
		let prev = f.held.swap(held, Ordering::SeqCst);
		if prev && !held {
			f.notify.notify_waiters();
		}
	}

	pub async fn wait_until_free(&self, key: &str) {
		loop {
			let f = self.floor(key);
			if !f.held.load(Ordering::SeqCst) {
				return;
			}
			f.notify.notified().await;
		}
	}
}
