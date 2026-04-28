//! `POST /flowgraph/{instance_id}/trigger/{node_id}` — Control からの安全なノード 1-shot 発火（Phase φ-2）。

use actix_web::web::{self, Data, Json};
use actix_web::{post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::flowgraph::node::{json_to_socket_value, InputMap, PortDirection, PortSpec, TriggerEvent};
use crate::flowgraph::registry;
use crate::SharedState;

use super::util::err_json;

/// リクエスト: `{"inputs": {"source": ..., ...}, "exec_port": "exec_in"?}`
///
/// `inputs` の value は以下いずれかを受け付ける:
///   - 生 JSON: `"ドクターウサギ"` / `42` / `true` / `["a","b"]`
///   - 型注釈版: `{"type":"string","value":"ドクターウサギ"}` — `type` は診断用で実際には
///     socket 側の `PortSpec.ty` が正とする。`value` だけを抽出して生 JSON 版と同じ経路に流す。
#[derive(Debug, Deserialize)]
pub struct TriggerNodeRequest {
	#[serde(default)]
	pub inputs: serde_json::Map<String, serde_json::Value>,
	/// 複数の exec 入力を持つノードで「どのポートを発火したか」を明示する場合に指定。
	/// 省略時は最初の exec 入力ポートを選ぶ（通常 `exec_in`）。
	#[serde(default)]
	pub exec_port: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TriggerNodeResponse {
	/// 処理受理（engine への送信まで）を表す。実行完了ではない点に注意。
	pub accepted: bool,
	pub instance_id: String,
	pub node_id: String,
	pub feature: String,
	pub exec_port: String,
	/// 実際に coerce して送った data override のキー名一覧。debug 用。
	pub overridden_inputs: Vec<String>,
}

/// `{"type":"T","value":V}` 形式の薄皮を剥がす。見つからなければそのまま返す。
fn unwrap_typed_value(v: &serde_json::Value) -> &serde_json::Value {
	if let serde_json::Value::Object(obj) = v {
		if obj.len() == 2 && obj.contains_key("type") && obj.contains_key("value") {
			if let Some(inner) = obj.get("value") {
				return inner;
			}
		}
	}
	v
}

/// 検証フェーズのエラー。HTTP status / error code / detail のタプルに落として返す。
///
/// 純粋関数部だけを切り出して `#[cfg(test)]` からも直接叩けるようにしている。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TriggerValidationError {
	NoExecInput,
	ExecPortNotFound(String),
	ExecPortNotExec(String),
	UnknownPort(String),
	ExecPortInData(String),
	InputTypeMismatch { port: String, ty: String },
}

impl TriggerValidationError {
	pub(crate) fn status(&self) -> actix_web::http::StatusCode {
		use actix_web::http::StatusCode;
		match self {
			Self::NoExecInput | Self::ExecPortNotExec(_) | Self::UnknownPort(_) | Self::ExecPortInData(_) => StatusCode::BAD_REQUEST,
			Self::ExecPortNotFound(_) => StatusCode::NOT_FOUND,
			Self::InputTypeMismatch { .. } => StatusCode::UNPROCESSABLE_ENTITY,
		}
	}

	pub(crate) fn code(&self) -> &'static str {
		match self {
			Self::NoExecInput => "no_exec_input",
			Self::ExecPortNotFound(_) => "exec_port_not_found",
			Self::ExecPortNotExec(_) => "invalid_port",
			Self::UnknownPort(_) => "unknown_port",
			Self::ExecPortInData(_) => "invalid_port",
			Self::InputTypeMismatch { .. } => "input_type_mismatch",
		}
	}

	fn detail(&self) -> String {
		match self {
			Self::NoExecInput => "ノードに exec 入力ポートがありません".into(),
			Self::ExecPortNotFound(n) => format!("ノードに exec_port '{n}' がありません"),
			Self::ExecPortNotExec(n) => {
				format!("指定された exec_port '{n}' は exec 入力ではありません")
			}
			Self::UnknownPort(n) => format!("ノードに入力ポート '{n}' が存在しません"),
			Self::ExecPortInData(n) => {
				format!("exec 入力 '{n}' に data 値は送れません（exec_port で指定）")
			}
			Self::InputTypeMismatch { port, ty } => {
				format!("入力 '{port}' の値を socket 型 {ty} に変換できません")
			}
		}
	}

	fn to_response(&self) -> HttpResponse {
		err_json(self.status(), self.code(), self.detail())
	}
}

fn select_exec_port<'a>(
	spec: &'a crate::flowgraph::node::NodeSpec,
	override_name: Option<&str>,
) -> Result<&'a PortSpec, TriggerValidationError> {
	if let Some(name) = override_name {
		match spec.inputs.iter().find(|p| p.name == name) {
			Some(p) if p.direction == PortDirection::Input && p.is_exec => Ok(p),
			Some(_) => Err(TriggerValidationError::ExecPortNotExec(name.to_string())),
			None => Err(TriggerValidationError::ExecPortNotFound(name.to_string())),
		}
	} else {
		spec.inputs
			.iter()
			.find(|p| p.direction == PortDirection::Input && p.is_exec)
			.ok_or(TriggerValidationError::NoExecInput)
	}
}

/// body.inputs を NodeSpec.inputs に沿って coerce する。exec ポートへの data 送信は拒否。
///
/// 純粋関数。HTTP / SharedState に依存しない。
pub(crate) fn coerce_input_overrides(
	spec: &crate::flowgraph::node::NodeSpec,
	inputs: &serde_json::Map<String, serde_json::Value>,
) -> Result<(InputMap, Vec<String>), TriggerValidationError> {
	let mut overrides = InputMap::new();
	let mut names: Vec<String> = Vec::with_capacity(inputs.len());
	for (name, raw_value) in inputs.iter() {
		let Some(port) = spec.inputs.iter().find(|p| &p.name == name) else {
			return Err(TriggerValidationError::UnknownPort(name.clone()));
		};
		if port.is_exec {
			return Err(TriggerValidationError::ExecPortInData(name.clone()));
		}
		let unwrapped = unwrap_typed_value(raw_value);
		let Some(sv) = json_to_socket_value(&port.ty, unwrapped) else {
			return Err(TriggerValidationError::InputTypeMismatch {
				port: name.clone(),
				ty: port.ty.to_string(),
			});
		};
		overrides.insert(name.clone(), sv);
		names.push(name.clone());
	}
	Ok((overrides, names))
}

#[post("/flowgraph/{instance_id}/trigger/{node_id}")]
pub async fn post_trigger_node(
	state: Data<SharedState>,
	path: web::Path<(String, String)>,
	body: Json<TriggerNodeRequest>,
) -> impl Responder {
	let (instance_id, node_id) = path.into_inner();

	if instance_id != "default" {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"instance_not_found",
			format!("instance_id '{instance_id}' は存在しません（V2 は 'default' のみ対応）"),
		);
	}

	let fg = state.read().await.flowgraph.clone();
	let rt_guard = fg.read().await;
	let Some(rt) = rt_guard.as_ref() else {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"instance_not_found",
			"flowgraph_dir が未設定または未ロードです",
		);
	};

	let Some(meta) = rt.node_meta.get(&node_id) else {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"node_not_found",
			format!("node_id '{node_id}' は存在しません"),
		);
	};
	let feature = meta.feature.clone();

	let reg = registry::registry();
	let Some(spec) = reg.spec(&feature) else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"feature_not_registered",
			format!("feature '{feature}' が registry に見つかりません（loader と不整合）"),
		);
	};

	if !reg.is_control_triggerable(&feature) {
		return err_json(
			actix_web::http::StatusCode::FORBIDDEN,
			"feature_not_triggerable",
			format!("feature '{feature}' は control API からの外部トリガに opt-in していません"),
		);
	}

	let exec_port = match select_exec_port(&spec, body.exec_port.as_deref()) {
		Ok(p) => p.name.clone(),
		Err(e) => return e.to_response(),
	};

	let (overrides, overridden_names) = match coerce_input_overrides(&spec, &body.inputs) {
		Ok(v) => v,
		Err(e) => return e.to_response(),
	};

	let Some(handle) = rt.trigger() else {
		return err_json(
			actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
			"worker_not_running",
			"Flowgraph worker が停止中です（ロードエラー等）",
		);
	};

	let mut event = TriggerEvent::new(node_id.clone()).with_exec(exec_port.clone());
	event.data_overrides = overrides;

	if let Err(e) = handle.send(event) {
		return err_json(
			actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
			"trigger_send_failed",
			format!("trigger 送信失敗: {e}"),
		);
	}

	log::info!(
		"《Flowgraph/Trigger》 {} :: {} (feature={}, exec={}, overrides={})",
		instance_id,
		node_id,
		feature,
		exec_port,
		overridden_names.len()
	);

	HttpResponse::Accepted().json(TriggerNodeResponse {
		accepted: true,
		instance_id,
		node_id,
		feature,
		exec_port,
		overridden_inputs: overridden_names,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::socket::SocketValue;
	use serde_json::json;

	fn learn_spec() -> crate::flowgraph::node::NodeSpec {
		registry::registry()
			.spec("flowgraph.glossary.learn")
			.expect("learn が registry に必要")
	}

	#[test]
	fn unwrap_typed_value_strips_wrapper() {
		let wrapped = json!({ "type": "string", "value": "DR.USAGI" });
		assert_eq!(unwrap_typed_value(&wrapped), &json!("DR.USAGI"));

		let bare = json!("DR.USAGI");
		assert_eq!(unwrap_typed_value(&bare), &bare);

		// 余計なキーが付いていたらそのまま（Json ポート向け）。
		let extra = json!({ "type": "string", "value": "x", "extra": 1 });
		assert_eq!(unwrap_typed_value(&extra), &extra);
	}

	#[test]
	fn select_exec_port_picks_first_by_default() {
		let spec = learn_spec();
		let p = select_exec_port(&spec, None).expect("learn は exec_in を持つ");
		assert_eq!(p.name, "exec_in");
		assert!(p.is_exec);
	}

	#[test]
	fn select_exec_port_accepts_matching_override() {
		let spec = learn_spec();
		let p = select_exec_port(&spec, Some("exec_in")).unwrap();
		assert_eq!(p.name, "exec_in");
	}

	#[test]
	fn select_exec_port_rejects_non_exec_override() {
		let spec = learn_spec();
		// `source` は String 入力であって exec ではない。
		let err = select_exec_port(&spec, Some("source")).unwrap_err();
		assert!(matches!(err, TriggerValidationError::ExecPortNotExec(ref n) if n == "source"));
		assert_eq!(err.status(), actix_web::http::StatusCode::BAD_REQUEST);
		assert_eq!(err.code(), "invalid_port");
	}

	#[test]
	fn select_exec_port_rejects_missing_override() {
		let spec = learn_spec();
		let err = select_exec_port(&spec, Some("ghost")).unwrap_err();
		assert!(matches!(err, TriggerValidationError::ExecPortNotFound(_)));
		assert_eq!(err.status(), actix_web::http::StatusCode::NOT_FOUND);
	}

	#[test]
	fn coerce_input_overrides_happy_path() {
		let spec = learn_spec();
		let inputs = serde_json::Map::from_iter([
			("source".into(), json!("ドクターウサギ")),
			("replacement".into(), json!({"type":"string","value":"DR.USAGI"})),
			("priority".into(), json!(3)),
		]);
		let (map, names) = coerce_input_overrides(&spec, &inputs).unwrap();
		assert_eq!(names.len(), 3);
		assert!(matches!(map.get("source"), Some(SocketValue::String(s)) if s == "ドクターウサギ"));
		assert!(matches!(map.get("replacement"), Some(SocketValue::String(s)) if s == "DR.USAGI"));
		assert!(matches!(map.get("priority"), Some(SocketValue::Int(3))));
	}

	#[test]
	fn coerce_input_overrides_unknown_port() {
		let spec = learn_spec();
		let inputs = serde_json::Map::from_iter([("ghost".into(), json!("x"))]);
		let err = coerce_input_overrides(&spec, &inputs).unwrap_err();
		assert!(matches!(err, TriggerValidationError::UnknownPort(ref n) if n == "ghost"));
		assert_eq!(err.status(), actix_web::http::StatusCode::BAD_REQUEST);
		assert_eq!(err.code(), "unknown_port");
	}

	#[test]
	fn coerce_input_overrides_exec_port_rejected() {
		let spec = learn_spec();
		let inputs = serde_json::Map::from_iter([("exec_in".into(), json!(true))]);
		let err = coerce_input_overrides(&spec, &inputs).unwrap_err();
		assert!(matches!(err, TriggerValidationError::ExecPortInData(_)));
	}

	#[test]
	fn coerce_input_overrides_type_mismatch() {
		let spec = learn_spec();
		// priority は int 要求だが string を投げる → 422。
		let inputs = serde_json::Map::from_iter([("priority".into(), json!("not-a-number"))]);
		let err = coerce_input_overrides(&spec, &inputs).unwrap_err();
		assert!(matches!(err, TriggerValidationError::InputTypeMismatch { .. }));
		assert_eq!(err.status(), actix_web::http::StatusCode::UNPROCESSABLE_ENTITY);
		assert_eq!(err.code(), "input_type_mismatch");
	}

	#[test]
	fn coerce_input_overrides_empty_is_ok() {
		let spec = learn_spec();
		let (map, names) = coerce_input_overrides(&spec, &serde_json::Map::new()).unwrap();
		assert!(map.is_empty());
		assert!(names.is_empty());
	}
}
