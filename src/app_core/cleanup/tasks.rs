use crate::{motion, processor};

pub(super) async fn finish_motion_handles(motion_handles: motion::MotionHandles) {
	motion_handles.finish_all().await;
}

pub(super) fn abort_ingress_handles(handles: processor::ingress::IngressHandles) {
	for h in handles.eventsub {
		h.abort();
	}
}

pub(super) fn abort_ai_handles(handles: Vec<tokio::task::JoinHandle<()>>) {
	for h in handles {
		h.abort();
	}
}
