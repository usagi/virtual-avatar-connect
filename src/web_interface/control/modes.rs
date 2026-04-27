//! RM-3 / RM-2 / RM-5 先取り: Runtime Mode の列挙・現在値・切替・plan・transit（dry-run）。

use actix_web::web::{Data, Json};
use actix_web::{get, post, put, HttpResponse, Responder};

use crate::conf::{build_mode_transition_plan, Conf, ModeTransitionPlan};
use crate::state::apply_runtime_mode_change;
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

#[derive(Debug, serde::Deserialize)]
pub struct PlanBody {
	/// 遷移先スロット。`null` または省略で `default_runtime_mode` 相当。
	pub target: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransitBody {
	pub mode: Option<String>,
	#[serde(default)]
	pub dry_run: bool,
	pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct TransitResponse {
	pub dry_run: bool,
	pub mode: Option<String>,
	pub plan: ModeTransitionPlan,
}

fn normalize_body_mode(mode: Option<&String>) -> Option<String> {
	mode.and_then(|m| {
		let t = m.trim();
		if t.is_empty() {
			None
		} else {
			Some(t.to_string())
		}
	})
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
	let Ok(conf) = Conf::new_noop_probe(path) else {
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
	let Ok(conf) = Conf::new_noop_probe(path) else {
		return HttpResponse::InternalServerError().json(serde_json::json!({
			"error": "conf_load_failed",
			"message": "conf を再読込できませんでした"
		}));
	};

	let normalized = normalize_body_mode(body.mode.as_ref());
	match apply_runtime_mode_change(state.get_ref(), &conf, normalized, None).await {
		Ok(applied) => HttpResponse::Ok().json(CurrentModeResponse {
			mode: applied.mode_slot,
		}),
		Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
			"error": "unknown_mode",
			"message": e.to_string(),
		})),
	}
}

#[post("/modes/plan")]
pub async fn post_modes_plan(state: Data<SharedState>, body: Json<PlanBody>) -> impl Responder {
	let s = state.read().await;
	let Some(path) = s.conf_source_path.as_ref() else {
		return HttpResponse::ServiceUnavailable().json(serde_json::json!({
			"error": "conf_source_path_unset",
			"message": "conf の出自パスが無いため plan を計算できません"
		}));
	};
	let Ok(conf) = Conf::new_noop_probe(path) else {
		return HttpResponse::InternalServerError().json(serde_json::json!({
			"error": "conf_load_failed",
			"message": "conf を再読込できませんでした"
		}));
	};
	let current = s
		.runtime_mode_id
		.read()
		.ok()
		.and_then(|g| g.clone());
	let target_norm = normalize_body_mode(body.target.as_ref());
	match build_mode_transition_plan(&conf, current.as_deref(), target_norm.as_deref()) {
		Ok(plan) => HttpResponse::Ok().json(plan),
		Err(msg) => HttpResponse::BadRequest().json(serde_json::json!({
			"error": "plan_failed",
			"message": msg,
		})),
	}
}

#[post("/modes/transit")]
pub async fn post_modes_transit(state: Data<SharedState>, body: Json<TransitBody>) -> impl Responder {
	let s = state.read().await;
	let Some(path) = s.conf_source_path.as_ref() else {
		return HttpResponse::ServiceUnavailable().json(serde_json::json!({
			"error": "conf_source_path_unset",
			"message": "conf の出自パスが無いため transit できません"
		}));
	};
	let Ok(conf) = Conf::new_noop_probe(path) else {
		return HttpResponse::InternalServerError().json(serde_json::json!({
			"error": "conf_load_failed",
			"message": "conf を再読込できませんでした"
		}));
	};

	let current = s
		.runtime_mode_id
		.read()
		.ok()
		.and_then(|g| g.clone());
	let normalized = normalize_body_mode(body.mode.as_ref());

	let plan = match build_mode_transition_plan(&conf, current.as_deref(), normalized.as_deref()) {
		Ok(p) => p,
		Err(msg) => {
			return HttpResponse::BadRequest().json(serde_json::json!({
				"error": "plan_failed",
				"message": msg,
			}));
		}
	};

	if body.dry_run {
		return HttpResponse::Ok().json(TransitResponse {
			dry_run: true,
			mode: current,
			plan,
		});
	}

	let reason = body
		.reason
		.as_ref()
		.map(|s| s.trim().to_string())
		.filter(|s| !s.is_empty());

	match apply_runtime_mode_change(state.get_ref(), &conf, normalized.clone(), reason).await {
		Ok(applied) => {
			let mode_for_json = applied.mode_slot.clone();
			HttpResponse::Ok().json(TransitResponse {
				dry_run: false,
				mode: mode_for_json,
				plan,
			})
		}
		Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
			"error": "unknown_mode",
			"message": e.to_string(),
		})),
	}
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
	cfg.service(get_modes_list)
		.service(get_current_mode)
		.service(put_current_mode)
		.service(post_modes_plan)
		.service(post_modes_transit);
}
