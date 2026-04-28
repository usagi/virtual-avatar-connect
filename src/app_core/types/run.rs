use crate::{shutdown, Result};
use std::sync::Arc;

pub(crate) struct AppCoreRuntimeHandle {
	pub(super) gui_url: String,
	pub(super) shutdown: Arc<shutdown::ShutdownBroker>,
}

pub(crate) struct AppCoreRunResult {
	serve: Result<()>,
	cleanup: Result<()>,
}

impl AppCoreRunResult {
	pub(crate) fn new(serve: Result<()>, cleanup: Result<()>) -> Self {
		Self { serve, cleanup }
	}

	pub(crate) fn into_parts(self) -> (Result<()>, Result<()>) {
		(self.serve, self.cleanup)
	}

	pub(crate) fn into_result(self) -> Result<()> {
		self.serve?;
		self.cleanup
	}
}

impl AppCoreRuntimeHandle {
	pub(crate) fn new(gui_url: String, shutdown: Arc<shutdown::ShutdownBroker>) -> Self {
		Self { gui_url, shutdown }
	}

	pub(crate) fn into_parts(self) -> (String, Arc<shutdown::ShutdownBroker>) {
		(self.gui_url, self.shutdown)
	}
}
