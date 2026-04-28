mod parts;
mod run;

pub(in crate::app_core) use parts::{AppCoreParts, AppCoreServices, AppCoreTasks};
pub(crate) use run::{AppCoreRunResult, AppCoreRuntimeHandle};
