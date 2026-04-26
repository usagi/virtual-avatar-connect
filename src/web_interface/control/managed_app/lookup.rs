//! Managed Apps API の registry 参照ヘルパ。

use actix_web::HttpResponse;

use crate::managed_app::{ManagedAppSpec, ManagedAppStatus};
use crate::SharedState;

/// 指定 ID の spec を引いて `RunWith` clone + 現在 status を返す共通前処理。
/// 見つからなければ 404 を返す `HttpResponse` を Err 側に包む。
pub(super) async fn lookup_entry(
	state: &SharedState,
	id: &str,
) -> Result<(crate::conf::RunWith, ManagedAppStatus), HttpResponse> {
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

pub(super) async fn lookup_spec_status(
	state: &SharedState,
	id: &str,
) -> Result<(ManagedAppSpec, ManagedAppStatus), HttpResponse> {
	let registry = state.read().await.managed_apps.clone();
	let r = registry.read().await;
	let spec = r
		.find_spec(id)
		.cloned()
		.ok_or_else(|| HttpResponse::NotFound().json(serde_json::json!({"error": "not_found", "id": id})))?;
	let status = r.statuses.get(id).cloned().unwrap_or_else(|| ManagedAppStatus::unknown(id));
	Ok((spec, status))
}
