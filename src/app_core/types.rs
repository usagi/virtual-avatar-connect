use crate::{shutdown, Result};
use std::sync::Arc;

pub(crate) struct AppCoreRuntimeHandle {
	pub(crate) gui_url: String,
	pub(crate) shutdown: Arc<shutdown::ShutdownBroker>,
}

pub(crate) struct AppCoreRunResult {
	pub(crate) serve: Result<()>,
	pub(crate) cleanup: Result<()>,
}

impl AppCoreRunResult {
	pub(crate) fn into_result(self) -> Result<()> {
		self.serve?;
		self.cleanup
	}
}
