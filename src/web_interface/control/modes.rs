//! RM-3 / RM-2 先取り: Runtime Mode の列挙・現在値・切替（exec ゲート再計算）。

use actix_web::web::{Data, Json};
use actix_web::{get, put, HttpResponse, Responder};

use crate::SharedState;

#[derive(Debug, serde::Serialize)]
pub struct ModesListResponse {
	/// `conf.modes` のキー（昇順）。
	pub mode_ids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct CurrentModeResponse {
	/// 現在選択。`null` は `conf.default_runtime_mode` に従う。
	pub mode: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct PutCurrentModeBody {
	/// 切替先。`null` で `default_runtime_mode` 相当（State 上は `None`）。
	pub mode: Option<String>,
}

#[get("/modes")]
pub async fn get_modes_list(state: Data<SharedState>) -> impl Responder {
	let s = state.read().await;
	let Some(path) = s.conf_source_path.as_ref() else {
		return HttpResponse::ServiceUnavailable().json(serde_json::json!({
			"error": "conf_source_path_unset",
			"message": "conf の出自パスが無いため modes を列挙できません"
		}));
	};
	let Ok(conf) = crate::conf::Conf::new_noop_probe(path) else {
		return HttpResponse::InternalServerError().json(serde_json::json!({
			"error": "conf_load_failed",
			"message": "conf を再読込できませんでした"
		}));
	};
	let mut mode_ids: Vec<String> = conf.modes.keys().cloned().collect();
	mode_ids.sort();
	HttpResponse::Ok().json(ModesListResponse { mode_ids })
}

#[get("/modes/current")]
pub async fn get_current_mode(state: Data<SharedState>) -> impl Responder {
	let s = state.read().await;
	let mode = s
		.runtime_mode_id
		.read()
		.ok()
		.and_then(|g| g.clone());
	HttpResponse::Ok().json(CurrentModeResponse { mode })
}

#[put("/modes/current")]
pub async fn put_current_mode(state: Data<SharedState>, body: Json<PutCurrentModeBody>) -> impl Responder {
	let s = state.read().await;
	let Some(path) = s.conf_source_path.as_ref() else {
		return HttpResponse::ServiceUnavailable().json(serde_json::json!({
			"error": "conf_source_path_unset",
			"message": "conf の出自パスが無いため mode を切り替えられません"
		}));
	};
	let Ok(conf) = crate::conf::Conf::new_noop_probe(path) else {
		return HttpResponse::InternalServerError().json(serde_json::json!({
			"error": "conf_load_failed",
			"message": "conf を再読込できませんでした"
		}));
	};

	let normalized: Option<String> = body.mode.as_ref().and_then(|m| {
		let t = m.trim();
		if t.is_empty() {
			None
		} else {
			Some(t.to_string())
		}
	});

	if let Some(ref m) = normalized {
		if !conf.modes.is_empty() && !conf.modes.contains_key(m) {
			return HttpResponse::BadRequest().json(serde_json::json!({
				"error": "unknown_mode",
				"message": format!("modes に '{m}' が存在しません")
			}));
		}
	}

	let mode_for_response: Option<String> = {
		if let Ok(mut slot) = s.runtime_mode_id.write() {
			*slot = normalized;
		}
		s.runtime_mode_id.read().ok().and_then(|g| g.clone())
	};

	let fg = s.flowgraph.read().await;
	if let Some(rt) = fg.as_ref() {
		rt.recompute_trigger_gate(&conf, mode_for_response.as_deref());
	}

	HttpResponse::Ok().json(CurrentModeResponse {
		mode: mode_for_response,
	})
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
	cfg.service(get_modes_list)
		.service(get_current_mode)
		.service(put_current_mode);
}
