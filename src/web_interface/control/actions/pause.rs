//! AI persona の pause / resume 適用。

use actix_web::HttpResponse;

use crate::SharedState;

use crate::web_interface::control::events::ControlEvent;

use super::PauseTarget;

pub(super) async fn apply_pause(state: &SharedState, target: &PauseTarget, paused: bool) -> HttpResponse {
	let s = state.read().await;

	let mut ais_affected: Vec<usize> = Vec::new();

	match target {
		PauseTarget::Ais => {
			let guard = s.ai_runtimes.read().await;
			for (i, rt) in guard.iter().enumerate() {
				rt.set_paused(paused);
				ais_affected.push(i);
			}
		}
		PauseTarget::Ai { id, index } => {
			if id.is_none() && index.is_none() {
				return bad_request("ai target requires either `id` or `index`");
			}
			let guard = s.ai_runtimes.read().await;
			let found = resolve_ai_index(&guard, id.as_deref(), *index);
			match found {
				Ok(i) => {
					guard[i].set_paused(paused);
					ais_affected.push(i);
				}
				Err(msg) => return bad_request(&msg),
			}
		}
	}

	log::info!(
		"《ControlAPI》 {} 適用: target={:?} ais={:?}",
		if paused { "pause" } else { "resume" },
		target,
		ais_affected
	);

	let target_tag: &'static str = match target {
		PauseTarget::Ais => "ais",
		PauseTarget::Ai { .. } => "ai",
	};
	let _ = s.control_event_tx.send(ControlEvent::PauseState {
		paused,
		target: target_tag,
		processors_affected: vec![],
		ais_affected: ais_affected.clone(),
	});

	HttpResponse::Ok().json(super::PauseOutcome { ais_affected, paused })
}

pub(super) fn resolve_ai_index(runtimes: &[crate::state::AiRuntime], id: Option<&str>, index: Option<usize>) -> Result<usize, String> {
	if let Some(wanted) = id {
		for (i, rt) in runtimes.iter().enumerate() {
			if rt.persona_id.as_deref() == Some(wanted) {
				return Ok(i);
			}
		}
		return Err(format!("ai persona id {:?} は登録されていません。", wanted));
	}
	if let Some(i) = index {
		if i < runtimes.len() {
			return Ok(i);
		}
		return Err(format!("ai index {} は範囲外です（有効: 0..{}）。", i, runtimes.len()));
	}
	Err("ai target requires either `id` or `index`".to_string())
}

fn bad_request(reason: &str) -> HttpResponse {
	HttpResponse::BadRequest()
		.content_type("application/json")
		.body(format!(r#"{{"error":"bad_request","reason":"{}"}}"#, reason.replace('"', "\\\"")))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::state::AiRuntime;

	fn mkai(id: Option<&str>) -> AiRuntime {
		AiRuntime::new(id.map(|s| s.to_string()))
	}

	#[test]
	fn resolve_ai_by_id() {
		let rts = vec![mkai(Some("kaltsit")), mkai(Some("sesa"))];
		let i = resolve_ai_index(&rts, Some("sesa"), None).unwrap();
		assert_eq!(i, 1);
	}

	#[test]
	fn resolve_ai_unknown_id() {
		let rts = vec![mkai(Some("kaltsit"))];
		let err = resolve_ai_index(&rts, Some("nobody"), None).unwrap_err();
		assert!(err.contains("登録されていません"), "unexpected msg: {err}");
	}

	#[test]
	fn ai_runtime_pause_toggle_is_shared() {
		let rt = mkai(Some("kaltsit"));
		let clone = rt.clone();
		assert!(!rt.is_paused());
		clone.set_paused(true);
		assert!(rt.is_paused());
	}
}
