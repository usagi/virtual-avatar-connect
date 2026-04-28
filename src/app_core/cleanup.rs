use super::types::AppCoreTasks;
use super::AppCoreParts;
use crate::Result;
use state::{finish_bridge_handles, stop_libretranslate, stop_managed_apps};
use tasks::{abort_ai_handles, abort_ingress_handles, finish_motion_handles};

mod state;
mod tasks;

pub(super) async fn cleanup(parts: AppCoreParts) -> Result<()> {
	let state = parts.state.clone();
	let AppCoreTasks {
		ai_handles,
		ingress_handles,
		motion_handles,
	} = parts.tasks;

	stop_managed_apps(&state).await;

	finish_motion_handles(motion_handles).await;

	finish_bridge_handles(&state).await;
	stop_libretranslate(&state).await;
	abort_ingress_handles(ingress_handles);
	abort_ai_handles(ai_handles);

	log::info!("《Shutdown》 cleanup 完了。プロセスを終了します。");
	Ok(())
}
