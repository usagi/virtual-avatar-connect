//! Phase M0: **motion 層** — VMC 生 UDP パススルー（Flowgraph 非依存）。
//!
//! 設計の正本: [`docs/roadmap/v2-vmc-and-restructure.md`](../../docs/roadmap/v2-vmc-and-restructure.md)  
//! M0 詳細: [`docs/roadmap/phase-mu-vmc-motion-m0.md`](../../docs/roadmap/phase-mu-vmc-motion-m0.md)

mod osc;
mod router;
mod vmc_raw;

use crate::conf::Conf;
use crate::shutdown::ShutdownBroker;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// 起動中の motion タスク。`run()` の cleanup で [`MotionHandles::finish_all`] する。
pub struct MotionHandles {
	tasks: Vec<JoinHandle<()>>,
}

impl MotionHandles {
	pub fn empty() -> Self {
		Self { tasks: Vec::new() }
	}

	pub fn is_empty(&self) -> bool {
		self.tasks.is_empty()
	}

	/// `conf.motion` に従い VMC passthrough タスクを spawn する。
	pub fn spawn_all(conf: &Conf, shutdown: Arc<ShutdownBroker>) -> Self {
		let Some(m) = conf.motion.as_ref() else {
			return Self::empty();
		};
		let mut tasks = Vec::new();
		for spec in &m.vmc_passthrough {
			if let Some(h) = vmc_raw::try_spawn(spec, shutdown.clone()) {
				tasks.push(h);
			}
		}
		if !tasks.is_empty() {
			log::info!("《Motion》 ワーカー {} 本起動（VMC passthrough）", tasks.len());
		}
		Self { tasks }
	}

	/// 全タスクを中止し、終了を待つ（cleanup 用）。
	pub async fn finish_all(self) {
		for h in self.tasks {
			h.abort();
			let _ = h.await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn motion_handles_default_empty() {
		let h = MotionHandles::empty();
		assert!(h.is_empty());
	}
}
