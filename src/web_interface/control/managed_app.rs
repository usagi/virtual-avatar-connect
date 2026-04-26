//! Phase VI-γ-2b: Managed App 操作 API。
//!
//! 提供エンドポイント:
//!   - `GET  /api/v1/control/managed_apps`           … specs + 現在 status
//!   - `POST /api/v1/control/managed_apps/:id/start`   … 起動（既に running なら 409 Conflict）
//!   - `POST /api/v1/control/managed_apps/:id/stop`    … 2 段階停止（WM_CLOSE → grace → TerminateProcess、Windows 専用）
//!   - `POST /api/v1/control/managed_apps/:id/restart` … stop → start の連続操作（γ-2）
//!   - `POST /api/v1/control/managed_apps/:id/minimize`… 最小化（Windows 専用）
//!
//! 不明 `id` は 404、`supports_status=false` entry への stop/minimize は 400、
//! Windows 以外の stop/minimize は 501 を返す。

use actix_web::web::{self, Data, Json, Path};
use actix_web::{get, post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::managed_app::{self, ManagedAppSpec, ManagedAppStatus};
use crate::SharedState;

/// GET のレスポンス 1 件ぶん。spec と現在 status を 1 つにマージした GUI 都合の形。
#[derive(Debug, Serialize, Clone)]
pub struct ManagedAppView {
	#[serde(flatten)]
	pub spec: ManagedAppSpec,
	pub status: ManagedAppStatus,
}

#[derive(Debug, Serialize)]
pub struct ManagedAppsResponse {
	pub entries: Vec<ManagedAppView>,
}

#[derive(Debug, Deserialize, Default)]
pub struct StopRequest {
	/// 段階停止の待機時間（ミリ秒）。未指定なら spec の `shutdown.grace_ms`（conf）に従う。
	/// `conf` に何も指定がなければ既定 10000。`0` を指定するとクローズ通知送信直後にポーリングに入り、
	/// 200ms 以内に抜ける（事実上の「即 TerminateProcess」挙動に近い、ただし `CloseOnly` なら強制終了しない）。
	#[serde(default)]
	pub grace_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct StopResponse {
	pub id: String,
	pub closed_windows: usize,
	pub terminated_pids: usize,
}

#[derive(Debug, Serialize)]
pub struct MinimizeResponse {
	pub id: String,
	/// minimize スケジューリング対象になった root PID 数。実際にウィンドウが最小化されたかは非同期なので保証しない。
	pub scheduled_pids: usize,
}

#[derive(Debug, Serialize)]
pub struct StartResponse {
	pub id: String,
	/// 起動前の running 状態（true のまま変化なし → 409 相当。false → 起動試行）。
	pub was_running: bool,
}

#[get("/managed_apps")]
pub async fn get_managed_apps(state: Data<SharedState>) -> impl Responder {
	let registry = {
		let s = state.read().await;
		s.managed_apps.clone()
	};
	let r = registry.read().await;
	let entries: Vec<ManagedAppView> = r
		.specs
		.iter()
		.map(|spec| {
			let status = r
				.statuses
				.get(&spec.id)
				.cloned()
				.unwrap_or_else(|| ManagedAppStatus::unknown(&spec.id));
			ManagedAppView {
				spec: spec.clone(),
				status,
			}
		})
		.collect();
	HttpResponse::Ok().json(ManagedAppsResponse { entries })
}

#[post("/managed_apps/{id}/start")]
pub async fn post_start(state: Data<SharedState>, id: Path<String>) -> impl Responder {
	let id = id.into_inner();
	let (run_with_clone, current_status) = match lookup_entry(&state, &id).await {
		Ok(x) => x,
		Err(r) => return r,
	};

	// 既に running ならここで 409。supports_status=false の entry は status が常に false なので通る。
	if current_status.running {
		return HttpResponse::Conflict().json(serde_json::json!({
		 "error": "already_running",
		 "id": id,
		 "pids": current_status.pids,
		}));
	}

	log::info!("《ManagedApp》 id={:?} を起動します。", id);
	if let Err(e) = managed_app::start_entry(&run_with_clone) {
		log::error!("《ManagedApp》 id={:?} の起動に失敗: {e}", id);
		return HttpResponse::InternalServerError().json(serde_json::json!({
		 "error": "start_failed",
		 "id": id,
		 "detail": e.to_string(),
		}));
	}

	// 起動直後は sysinfo に反映されるまで数秒かかる。
	// 監視タスクが次回 tick で拾うのに任せる（即時 probe は行わない）。
	HttpResponse::Ok().json(StartResponse {
		id,
		was_running: current_status.running,
	})
}

#[post("/managed_apps/{id}/stop")]
pub async fn post_stop(state: Data<SharedState>, id: Path<String>, body: Option<Json<StopRequest>>) -> impl Responder {
	let id = id.into_inner();
	let body_grace_ms = body.and_then(|b| b.grace_ms);

	let (spec, status) = match lookup_spec_status(&state, &id).await {
		Ok(x) => x,
		Err(r) => return r,
	};

	if !spec.supports_status {
		return HttpResponse::BadRequest().json(serde_json::json!({
		 "error": "stop_unsupported",
		 "id": id,
		 "reason": "status 監視不可の entry は停止できません（`if_not_running` が未指定）。",
		}));
	}
	if !status.running {
		return HttpResponse::Ok().json(serde_json::json!({
		 "id": id,
		 "closed_windows": 0,
		 "terminated_pids": 0,
		 "note": "not_running",
		}));
	}

	// Phase ε-2: spec.shutdown をベースに、API の body.grace_ms で上書きできるようにする。
	// action / method は conf 由来のまま維持（途中で切替えられると debug しづらいため）。
	let cfg = match body_grace_ms {
		Some(g) => spec.shutdown.clone().with_grace_ms(g),
		None => spec.shutdown.clone(),
	};

	log::info!(
		"《ManagedApp》 id={:?} を停止します（action={:?} method={:?} grace_ms={} pids={:?}）。",
		id,
		cfg.action,
		cfg.method,
		cfg.grace_ms,
		status.pids
	);
	match managed_app::stop_entry_graceful(&status.pids, cfg).await {
		Ok(out) => {
			// 停止直後は sysinfo がまだ living pid を見ているかもしれない。即時 event を 1 発撃ってもよいが、
			// pids=空の確定まで監視タスクの次 tick に任せる方が一貫する（レイテンシ 3s 以内）。
			HttpResponse::Ok().json(StopResponse {
				id,
				closed_windows: out.closed_windows,
				terminated_pids: out.terminated_pids,
			})
		}
		Err(e) => {
			log::error!("《ManagedApp》 id={:?} の停止に失敗: {e}", id);
			// Windows 以外は bail されるため、501 に寄せる。
			#[cfg(target_os = "windows")]
			let st = actix_web::http::StatusCode::INTERNAL_SERVER_ERROR;
			#[cfg(not(target_os = "windows"))]
			let st = actix_web::http::StatusCode::NOT_IMPLEMENTED;
			HttpResponse::build(st).json(serde_json::json!({
			 "error": "stop_failed",
			 "id": id,
			 "detail": e.to_string(),
			}))
		}
	}
}

#[derive(Debug, Serialize)]
pub struct RestartResponse {
	pub id: String,
	pub closed_windows: usize,
	pub terminated_pids: usize,
	/// stop を試みた結果プロセスが走っていなかった場合は false（start のみ実行）。
	pub was_running: bool,
}

#[post("/managed_apps/{id}/restart")]
pub async fn post_restart(state: Data<SharedState>, id: Path<String>, body: Option<Json<StopRequest>>) -> impl Responder {
	let id = id.into_inner();
	let body_grace_ms = body.and_then(|b| b.grace_ms);

	// 1) stop 部分: spec / status を解決し、running なら graceful stop。supports_status=false は restart 非対応。
	let (spec, status) = match lookup_spec_status(&state, &id).await {
		Ok(x) => x,
		Err(r) => return r,
	};
	if !spec.supports_status {
		return HttpResponse::BadRequest().json(serde_json::json!({
			"error": "restart_unsupported",
			"id": id,
			"reason": "status 監視不可の entry は restart できません（`if_not_running` が未指定）。",
		}));
	}

	let (closed_windows, terminated_pids, was_running) = if status.running {
		let cfg = match body_grace_ms {
			Some(g) => spec.shutdown.clone().with_grace_ms(g),
			None => spec.shutdown.clone(),
		};
		log::info!(
			"《ManagedApp》 id={:?} を再起動します（stop action={:?} method={:?} grace_ms={} pids={:?}）。",
			id,
			cfg.action,
			cfg.method,
			cfg.grace_ms,
			status.pids
		);
		match managed_app::stop_entry_graceful(&status.pids, cfg).await {
			Ok(out) => (out.closed_windows, out.terminated_pids, true),
			Err(e) => {
				log::error!("《ManagedApp》 id={:?} 再起動中の停止に失敗: {e}", id);
				#[cfg(target_os = "windows")]
				let st = actix_web::http::StatusCode::INTERNAL_SERVER_ERROR;
				#[cfg(not(target_os = "windows"))]
				let st = actix_web::http::StatusCode::NOT_IMPLEMENTED;
				return HttpResponse::build(st).json(serde_json::json!({
					"error": "stop_failed",
					"id": id,
					"detail": e.to_string(),
				}));
			}
		}
	} else {
		(0, 0, false)
	};

	// 2) start 部分: spec → run_with 再構成して起動。stop_entry_graceful が終わった直後は sysinfo がまだ古い
	//    可能性があるが、start_entry は process_marker 再探索を行う（既に死んでいれば即新規起動）のでそのまま呼ぶ。
	let registry = state.read().await.managed_apps.clone();
	let run_with = {
		let r = registry.read().await;
		match r.find_spec(&id).cloned() {
			Some(spec) => crate::conf::RunWith::CommandIfProcessIsNotRunning {
				command: spec.command,
				if_not_running: spec.process_marker,
				run_as_admin: Some(spec.run_as_admin),
				working_dir: spec.working_dir,
				minimized: Some(spec.minimized),
				id: Some(spec.id),
				label: Some(spec.label),
				shutdown: None,
			},
			None => {
				return HttpResponse::NotFound().json(serde_json::json!({"error": "not_found", "id": id}));
			}
		}
	};

	log::info!("《ManagedApp》 id={:?} を再起動します（start phase）。", id);
	if let Err(e) = managed_app::start_entry(&run_with) {
		log::error!("《ManagedApp》 id={:?} 再起動後の起動に失敗: {e}", id);
		return HttpResponse::InternalServerError().json(serde_json::json!({
			"error": "start_failed",
			"id": id,
			"detail": e.to_string(),
		}));
	}

	HttpResponse::Ok().json(RestartResponse {
		id,
		closed_windows,
		terminated_pids,
		was_running,
	})
}

#[post("/managed_apps/{id}/minimize")]
pub async fn post_minimize(state: Data<SharedState>, id: Path<String>) -> impl Responder {
	let id = id.into_inner();
	let (spec, status) = match lookup_spec_status(&state, &id).await {
		Ok(x) => x,
		Err(r) => return r,
	};

	if !spec.supports_status {
		return HttpResponse::BadRequest().json(serde_json::json!({
		 "error": "minimize_unsupported",
		 "id": id,
		 "reason": "status 監視不可の entry は最小化できません（`if_not_running` が未指定）。",
		}));
	}
	if !status.running {
		return HttpResponse::Conflict().json(serde_json::json!({
		 "error": "not_running",
		 "id": id,
		}));
	}

	#[cfg(not(target_os = "windows"))]
	{
		let _ = &spec;
		return HttpResponse::NotImplemented().json(serde_json::json!({
		 "error": "minimize_unsupported_os",
		 "id": id,
		 "reason": "minimize 操作は Windows 専用です。",
		}));
	}

	#[cfg(target_os = "windows")]
	{
		let _ = &spec;
		log::info!("《ManagedApp》 id={:?} を最小化します（pids={:?}）。", id, status.pids);
		let scheduled = managed_app::minimize_pids(&status.pids);
		// 監視タスク経由で最小化は running 状態を変えないので、即時 event は送らない。
		HttpResponse::Ok().json(MinimizeResponse {
			id,
			scheduled_pids: scheduled,
		})
	}
}

/// 指定 ID の spec を引いて `RunWith` clone + 現在 status を返す共通前処理。
/// 見つからなければ 404 を返す `HttpResponse` を Err 側に包む。
async fn lookup_entry(state: &SharedState, id: &str) -> Result<(crate::conf::RunWith, ManagedAppStatus), HttpResponse> {
	// run_with は conf 側に置いてある。state 経由で conf を読むのは避け、ConfigReload が走ったときに齟齬が出ても
	// ランタイム側 registry と同期する方針にしたい。ただし現状 conf は lib.rs のローカルで手放していて State 経由では
	// 参照できないため、ここでは registry の spec の command / process_marker から近似再構成する:
	//   → `start` は run_with entry の詳細（run_as_admin, working_dir, minimized）が要るので、State に conf を
	//      持たせるか、conf snapshot を共有する必要がある。今はまだ conf を State に載せていないので、代替として
	//      run_with の snapshot を State に持たせる方向が正しい。
	//
	// 暫定実装: registry の spec から RunWith::CommandIfProcessIsNotRunning を再構成する（id/label は spec から）。
	// これは「最初に読み込んだ conf と API 操作時の conf が同じ」である限り同値。restart API 経由の conf 差し替え時は
	// プロセス自体が再起動するので不整合は残らない。
	let registry = state.read().await.managed_apps.clone();
	let r = registry.read().await;
	let spec = r
		.find_spec(id)
		.ok_or_else(|| HttpResponse::NotFound().json(serde_json::json!({"error": "not_found", "id": id})))?;
	let status = r.statuses.get(id).cloned().unwrap_or_else(|| ManagedAppStatus::unknown(id));

	// spec から run_with 再構成。shutdown は `start_entry` では使われないので None で良い。
	let run_with = crate::conf::RunWith::CommandIfProcessIsNotRunning {
		command: spec.command.clone(),
		if_not_running: spec.process_marker.clone(),
		run_as_admin: Some(spec.run_as_admin),
		working_dir: spec.working_dir.clone(),
		minimized: Some(spec.minimized),
		id: Some(spec.id.clone()),
		label: Some(spec.label.clone()),
		shutdown: None,
	};
	Ok((run_with, status))
}

async fn lookup_spec_status(state: &SharedState, id: &str) -> Result<(ManagedAppSpec, ManagedAppStatus), HttpResponse> {
	let registry = state.read().await.managed_apps.clone();
	let r = registry.read().await;
	let spec = r
		.find_spec(id)
		.cloned()
		.ok_or_else(|| HttpResponse::NotFound().json(serde_json::json!({"error": "not_found", "id": id})))?;
	let status = r.statuses.get(id).cloned().unwrap_or_else(|| ManagedAppStatus::unknown(id));
	Ok((spec, status))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(get_managed_apps)
		.service(post_start)
		.service(post_stop)
		.service(post_restart)
		.service(post_minimize);
}
