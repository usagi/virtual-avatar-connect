//! Step 7（再構造化）: 通常運転の **bootstrap + HTTP serve + shutdown cleanup**。
//!
//! `crate::run()` はロガー・CLI 特殊モード・conf ロードまでを担当し、本モジュールが
//! `ShutdownBroker` 以降の常駐ランタイム本体をまとめる。将来 `vac-app` crate へ移す際の境界の目印。

mod address;
mod boot;
mod cleanup;
mod server;
mod types;

use crate::conf::Conf;
use crate::{Result, SharedAudioSink};
use types::AppCoreParts;
pub(crate) use types::{AppCoreRunResult, AppCoreRuntimeHandle};

/// conf ロード済み・`run_with` 済みの状態から起動する VAC 常駐ランタイム本体。
///
/// CLI / desktop runner は、最終的にこの `boot` / `serve` / `cleanup`
/// 境界を共有する。現時点では `run_vac_application` が従来通り直列に呼ぶ。
pub(crate) struct AppCore {
	parts: AppCoreParts,
}

impl AppCore {
	pub(crate) async fn boot(conf: Conf, audio_sink: SharedAudioSink) -> Result<Self> {
		let parts = boot::boot(conf, audio_sink).await?;
		Ok(Self::from_parts(parts))
	}

	pub(crate) async fn run(self) -> AppCoreRunResult {
		let serve = self.serve().await;
		let cleanup = self.cleanup().await;
		AppCoreRunResult { serve, cleanup }
	}

	pub(crate) fn runtime_handle(&self) -> AppCoreRuntimeHandle {
		self.parts.runtime_handle()
	}

	async fn serve(&self) -> Result<()> {
		self.parts.serve().await
	}

	async fn cleanup(self) -> Result<()> {
		cleanup::cleanup(self.parts).await
	}

	fn from_parts(parts: AppCoreParts) -> Self {
		Self { parts }
	}
}

impl AppCoreParts {
	async fn serve(&self) -> Result<()> {
		server::run_services(self).await
	}
}
