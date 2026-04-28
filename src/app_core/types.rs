use super::address;

mod parts;
mod run;

pub(in crate::app_core) use parts::{AppCoreParts, AppCoreServices, AppCoreTasks};
pub(crate) use run::{AppCoreRunResult, AppCoreRuntimeHandle};

impl AppCoreParts {
	pub(super) fn runtime_handle(&self) -> AppCoreRuntimeHandle {
		let address = address::normalize_loopback_address(self.conf.get_web_ui_address());
		AppCoreRuntimeHandle::new(format!("http://{address}/gui/"), self.shutdown.clone())
	}
}
