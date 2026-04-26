//! AI persona の無停止リロード適用。

use actix_web::HttpResponse;

use crate::ai::AiReloadRequest;
use crate::web_interface::control::events::ControlEvent;
use crate::SharedState;

use super::ReloadResponse;

pub(super) async fn handle_ai_reload(state: &SharedState, id: Option<&str>, changes: AiReloadRequest) -> HttpResponse {
	if changes.is_empty() {
		return bad_request("ai_persona target requires at least one changeable field (custom_instructions / system_instructions_extra / heartbeat_enabled / decision_threshold)");
	}

	// reload handle を取り出す（見つけたら即座にロックを解放する）
	let s = state.read().await;
	let ar = s.ai_runtimes.clone();
	let tx = s.control_event_tx.clone();
	drop(s);

	let handle_opt = {
		let guard = ar.read().await;
		match id {
			Some(wanted) => guard
				.iter()
				.find(|r| r.persona_id.as_deref() == Some(wanted))
				.and_then(|r| r.reload_handle.clone()),
			None => {
				// id 省略時は「対象ペルソナが 1 件しかないなら」OK、複数いたら曖昧なのでエラー
				let candidates: Vec<_> = guard.iter().filter_map(|r| r.reload_handle.clone()).collect();
				match candidates.len() {
					0 => None,
					1 => Some(candidates.into_iter().next().unwrap()),
					_ => {
						return bad_request("ai_persona target has multiple candidates; please specify `id`");
					}
				}
			}
		}
	};

	let Some(handle) = handle_opt else {
		return not_found(&format!(
			"ai persona id={:?} not found (or reload handle not registered)",
			id.unwrap_or("-")
		));
	};

	match handle.apply(changes).await {
		Ok(report) => {
			log::info!(
    "《ControlAPI/reload》 ai_persona id={:?} 適用: custom_instructions={} system_instructions_extra={} heartbeat_enabled={} decision_threshold={} warnings={:?}",
    handle.persona_id,
    report.custom_instructions_changed,
    report.system_instructions_extra_changed,
    report.heartbeat_enabled_changed,
    report.decision_threshold_changed,
    report.warnings
   );
			let _ = tx.send(ControlEvent::Reloaded {
				target: "ai_persona",
				id: handle.persona_id.clone(),
				detail: serde_json::to_value(&report).unwrap_or(serde_json::Value::Null),
			});
			HttpResponse::Ok().json(ReloadResponse {
				target: "ai_persona",
				ai_report: Some(report),
			})
		}
		Err(e) => {
			log::error!("《ControlAPI/reload》 ai_persona reload 失敗: {e:?}");
			HttpResponse::InternalServerError().json(serde_json::json!({
			 "error": "ai_persona_reload_failed",
			 "reason": e.to_string(),
			}))
		}
	}
}

fn bad_request(reason: &str) -> HttpResponse {
	HttpResponse::BadRequest()
		.content_type("application/json")
		.body(format!(r#"{{"error":"bad_request","reason":"{}"}}"#, reason.replace('"', "\\\"")))
}

fn not_found(reason: &str) -> HttpResponse {
	HttpResponse::NotFound()
		.content_type("application/json")
		.body(format!(r#"{{"error":"not_found","reason":"{}"}}"#, reason.replace('"', "\\\"")))
}
