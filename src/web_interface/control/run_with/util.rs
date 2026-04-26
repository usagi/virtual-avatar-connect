//! HTTP エラー JSON と `RunWith` → `RunWithView` のビュー構築。

use actix_web::HttpResponse;

use crate::conf::RunWith;

use super::{RunWithDto, RunWithView};

pub(crate) fn err_response(status: actix_web::http::StatusCode, code: &str, detail: impl std::fmt::Display) -> HttpResponse {
	HttpResponse::build(status).json(serde_json::json!({
	 "error": code,
	 "detail": detail.to_string(),
	}))
}

pub(crate) fn build_views(run_with: &[RunWith]) -> Vec<RunWithView> {
	run_with
		.iter()
		.enumerate()
		.map(|(idx, r)| RunWithView {
			index: idx,
			effective_id: r.explicit_id().map(|s| s.to_string()).unwrap_or_else(|| format!("run-with-{idx}")),
			display_label: r.display_label().to_string(),
			supports_status: r.process_marker().is_some(),
			entry: RunWithDto::from(r),
		})
		.collect()
}
