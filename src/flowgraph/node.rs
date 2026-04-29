//! VAC Flowgraph のノード仕様と実行 trait（spec §3）。
//!
//! ## ハイブリッドモデル（spec §5）
//!
//! - **PureNode**: 副作用なし、状態なし、決定論的。副作用 I/O には直接触れない（`ExecCtx` なし）。
//!   `compute(&self, &PureEvalHost, props, inputs, fired) -> NodeOutput`。
//!   ランタイム観測は `PureEvalHost` 経由。pull 型 lazy、メモ化対象。
//! - **StatefulNode**: engine が World として保持する state slot を `&mut dyn Any` で借りる。
//!   I/O はしない。Delay / BoolState / Counter などが該当。
//! - **EffectfulNode**: I/O 可（`&mut ExecCtx` 受け取り）。必ず exec 発火経由でのみ評価される。
//!   Log / TTS / HTTP / Twitch 送信などが該当。
//!
//! ## 型による純粋性保証
//!
//! - `PureNode::compute` は `ExecCtx` を受け取らない（HTTP / ファイル等の副作用 I/O には触れない）。
//!   ランタイム観測は `PureEvalHost` 経由に限定する。
//! - `EffectfulNode` のみ `ExecCtx` を通じて副作用を起こせる。
//! - `StatefulNode` は `&mut Any` で内部状態に書けるが、engine の I/O 資源には触れない。

use crate::flowgraph::quantity::{parse_unit, Quantity};
use crate::flowgraph::socket::{parse_quantity_string, SocketType, SocketValue};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;

// ---------------------------------------------------------------------
// PortSpec
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
	Input,
	Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSpec {
	pub name: String,
	pub label: String,
	pub ty: SocketType,
	pub direction: PortDirection,
	#[serde(default)]
	pub is_exec: bool,
	#[serde(default)]
	pub optional: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub default: Option<SocketValueRepr>,
	#[serde(default)]
	pub multi: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	/// Phase λ: `SocketType::String` ポートに限り、取りうる文字列の閉集合（ワイヤ型は `string` のまま）。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub closed_string_variants: Option<Vec<String>>,
}

impl PortSpec {
	pub fn input(name: &str, label: &str, ty: SocketType) -> Self {
		Self {
			name: name.into(),
			label: label.into(),
			ty,
			direction: PortDirection::Input,
			is_exec: false,
			optional: false,
			default: None,
			multi: false,
			description: None,
			closed_string_variants: None,
		}
	}

	pub fn output(name: &str, label: &str, ty: SocketType) -> Self {
		Self {
			name: name.into(),
			label: label.into(),
			ty,
			direction: PortDirection::Output,
			is_exec: false,
			optional: false,
			default: None,
			multi: false,
			description: None,
			closed_string_variants: None,
		}
	}

	pub fn exec_input(name: &str, label: &str) -> Self {
		Self {
			name: name.into(),
			label: label.into(),
			ty: SocketType::Exec,
			direction: PortDirection::Input,
			is_exec: true,
			optional: false,
			default: None,
			multi: false,
			description: None,
			closed_string_variants: None,
		}
	}

	pub fn exec_output(name: &str, label: &str) -> Self {
		Self {
			name: name.into(),
			label: label.into(),
			ty: SocketType::Exec,
			direction: PortDirection::Output,
			is_exec: true,
			optional: false,
			default: None,
			multi: false,
			description: None,
			closed_string_variants: None,
		}
	}

	pub fn with_default(mut self, v: SocketValue) -> Self {
		self.default = Some(SocketValueRepr::from_value(&v));
		self.optional = true;
		self
	}

	pub fn with_optional(mut self) -> Self {
		self.optional = true;
		self
	}

	pub fn with_multi(mut self) -> Self {
		self.multi = true;
		self
	}

	/// Phase λ: 文字列ポートの閉集合（`SocketType::String` 向け）。
	pub fn with_closed_string_variants<I, S>(mut self, variants: I) -> Self
	where
		I: IntoIterator<Item = S>,
		S: Into<String>,
	{
		self.closed_string_variants = Some(variants.into_iter().map(Into::into).collect());
		self
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketValueRepr(pub serde_json::Value);

impl SocketValueRepr {
	pub fn from_value(v: &SocketValue) -> Self {
		Self(match v {
			SocketValue::Bool(b) => serde_json::json!(b),
			SocketValue::Int(i) => serde_json::json!(i),
			SocketValue::Float(f) => serde_json::json!(f),
			SocketValue::String(s) => serde_json::json!(s),
			SocketValue::Json(v) => v.clone(),
			SocketValue::List(xs) => {
				let arr: Vec<serde_json::Value> = xs.iter().map(|e| SocketValueRepr::from_value(e).0).collect();
				serde_json::Value::Array(arr)
			}
			SocketValue::Map(m) => {
				let obj: serde_json::Map<String, serde_json::Value> =
					m.iter().map(|(k, v)| (k.clone(), SocketValueRepr::from_value(v).0)).collect();
				serde_json::Value::Object(obj)
			}
			SocketValue::Table(t) => t.to_json_array(),
			SocketValue::Quantity(q) => quantity_to_json(q),
			// Phase π: DateTime は RFC3339 (Z suffix) 文字列として wire に載せる。
			// 受信側 `json_to_socket_value` はこの文字列を parse して DateTime に復元する。
			SocketValue::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
			SocketValue::MotionFrame(m) => serde_json::to_value(m).unwrap_or(serde_json::Value::Null),
		})
	}

	pub fn to_socket_value(&self, expected: &SocketType) -> Option<SocketValue> {
		json_to_socket_value(expected, &self.0)
	}
}

pub(crate) fn json_to_socket_value(ty: &SocketType, v: &serde_json::Value) -> Option<SocketValue> {
	use serde_json::Value as J;
	match (ty, v) {
		(SocketType::Bool, J::Bool(b)) => Some(SocketValue::Bool(*b)),
		(SocketType::Int, J::Number(n)) => n.as_i64().map(SocketValue::Int),
		(SocketType::Float, J::Number(n)) => n.as_f64().map(SocketValue::Float),
		(SocketType::String, J::String(s)) => Some(SocketValue::String(s.clone())),
		(SocketType::Json, any) => Some(SocketValue::Json(any.clone())),
		(SocketType::List(inner), J::Array(arr)) => {
			let mut out = Vec::with_capacity(arr.len());
			for e in arr {
				out.push(json_to_socket_value(inner, e)?);
			}
			Some(SocketValue::List(out))
		}
		(SocketType::Map(inner), J::Object(obj)) => {
			let mut out = std::collections::BTreeMap::new();
			for (k, v) in obj {
				out.insert(k.clone(), json_to_socket_value(inner, v)?);
			}
			Some(SocketValue::Map(out))
		}
		(SocketType::Table, J::Array(arr)) => crate::flowgraph::table::Table::from_json_array(arr, None)
			.ok()
			.map(SocketValue::Table),
		(SocketType::Quantity, J::Number(n)) => n.as_f64().map(|f| SocketValue::Quantity(Quantity::dimensionless(f))),
		(SocketType::Quantity, J::String(s)) => parse_quantity_string(s).ok().map(SocketValue::Quantity),
		(SocketType::Quantity, J::Object(obj)) => quantity_from_json_object(obj).map(SocketValue::Quantity),
		// Phase π: JSON 文字列 → DateTime。parse 失敗は `None`（呼び出し側で
		// `SocketValueRepr::to_socket_value` が default 値 fallback するなど既存挙動に合流）。
		(SocketType::DateTime, J::String(s)) => crate::datetime::DateTime::from_rfc3339(s).ok().map(SocketValue::DateTime),
		(SocketType::MotionFrame, v) => serde_json::from_value(v.clone()).ok().map(SocketValue::MotionFrame),
		_ => None,
	}
}

/// Quantity の JSON internal 形式（`{"value": f64, "unit": "m/s^2"}`）へのエンコード。
///
/// `SocketValueRepr` で Flowgraph 内や gui の wire 経路を通る際の正規表現。
/// 外部 IO（`flowgraph.json.stringify` 等）からは Phase ξ §6.4 に従って value のみを取り出す
/// pass-through を行うため、当該コンバータは `flowgraph.unit.*` ノードや loader/TOML 復元で
/// のみ使われる。
pub(crate) fn quantity_to_json(q: &Quantity) -> serde_json::Value {
	let unit_str = q.unit.canonical();
	serde_json::json!({
		"value": q.value,
		"unit": unit_str,
	})
}

/// `{"value": number, "unit": "<unit string>"}` 形式の JSON object を [`Quantity`] に復元。
///
/// `unit` フィールド不在は dimensionless 扱い、`unit` が文字列でない場合や unit パース失敗は `None`。
pub(crate) fn quantity_from_json_object(obj: &serde_json::Map<String, serde_json::Value>) -> Option<Quantity> {
	let value = match obj.get("value")? {
		serde_json::Value::Number(n) => n.as_f64()?,
		_ => return None,
	};
	let unit = match obj.get("unit") {
		Some(serde_json::Value::String(s)) => {
			let s = s.trim();
			if s.is_empty() {
				crate::flowgraph::quantity::Unit::dimensionless()
			} else {
				parse_unit(s).ok()?
			}
		}
		None => crate::flowgraph::quantity::Unit::dimensionless(),
		_ => return None,
	};
	Some(Quantity::of(value, unit))
}

// ---------------------------------------------------------------------
// PropertySpec
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySpec {
	pub name: String,
	pub label: String,
	pub ty: SocketType,
	pub default: SocketValueRepr,
	#[serde(default)]
	pub required: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub validator: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ui_hint: Option<String>,
	/// Phase ο-2: enum-style constraint. When set, the value must be one of these
	/// strings (ty = String expected). The GUI property editor renders a `<select>`
	/// dropdown; engine-side validation is still the node's responsibility
	/// (mismatched values typically become `NodeExecError::Generic`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub choices: Option<Vec<String>>,
}

impl PropertySpec {
	pub fn new(name: &str, label: &str, ty: SocketType, default: SocketValue) -> Self {
		Self {
			name: name.into(),
			label: label.into(),
			ty,
			default: SocketValueRepr::from_value(&default),
			required: false,
			description: None,
			validator: None,
			ui_hint: None,
			choices: None,
		}
	}
	pub fn required(mut self) -> Self {
		self.required = true;
		self
	}
	pub fn description(mut self, s: &str) -> Self {
		self.description = Some(s.into());
		self
	}
	/// Phase ο-2: enum-style constraint. See field docs.
	pub fn with_choices<I, S>(mut self, choices: I) -> Self
	where
		I: IntoIterator<Item = S>,
		S: Into<String>,
	{
		self.choices = Some(choices.into_iter().map(Into::into).collect());
		self
	}
}

// ---------------------------------------------------------------------
// NodeSpec
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSpec {
	pub feature: String,
	pub title: String,
	pub category: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub inputs: Vec<PortSpec>,
	pub outputs: Vec<PortSpec>,
	#[serde(default)]
	pub properties: Vec<PropertySpec>,
}

impl NodeSpec {
	pub fn find_input(&self, name: &str) -> Option<&PortSpec> {
		self.inputs.iter().find(|p| p.name == name)
	}
	pub fn find_output(&self, name: &str) -> Option<&PortSpec> {
		self.outputs.iter().find(|p| p.name == name)
	}
	pub fn find_property(&self, name: &str) -> Option<&PropertySpec> {
		self.properties.iter().find(|p| p.name == name)
	}
}

// ---------------------------------------------------------------------
// 実行時の入出力マップと文脈
// ---------------------------------------------------------------------

pub type OutputMap = HashMap<String, SocketValue>;
pub type InputMap = HashMap<String, SocketValue>;

// ---------------------------------------------------------------------
// 入力アクセサ（型安全、冗長な boilerplate を削減）
// ---------------------------------------------------------------------

/// `NodeExecError::TypeMismatch` を生成する簡易ヘルパ。
fn type_err(port: &str, expected: SocketType, actual: SocketType) -> NodeExecError {
	NodeExecError::TypeMismatch {
		port: port.into(),
		expected,
		actual,
	}
}

pub fn get_required_bool(inputs: &InputMap, key: &str) -> Result<bool, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_bool().map_err(|_| type_err(key, SocketType::Bool, v.type_of()))
}

pub fn get_required_int(inputs: &InputMap, key: &str) -> Result<i64, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_i64().map_err(|_| type_err(key, SocketType::Int, v.type_of()))
}

pub fn get_required_float(inputs: &InputMap, key: &str) -> Result<f64, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_f64().map_err(|_| type_err(key, SocketType::Float, v.type_of()))
}

/// Quantity を要求する入力取得。`flowgraph.unit.*` ノードで使用される。
pub fn get_required_quantity<'a>(inputs: &'a InputMap, key: &str) -> Result<&'a Quantity, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_quantity().map_err(|_| type_err(key, SocketType::Quantity, v.type_of()))
}

pub fn get_required_string(inputs: &InputMap, key: &str) -> Result<String, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_str()
		.map(str::to_owned)
		.map_err(|_| type_err(key, SocketType::String, v.type_of()))
}

/// Phase π-5: DateTime 入力取得。`flowgraph.datetime.*` ノードで使用。
pub fn get_required_datetime(inputs: &InputMap, key: &str) -> Result<crate::datetime::DateTime, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_datetime()
		.copied()
		.map_err(|_| type_err(key, SocketType::DateTime, v.type_of()))
}

pub fn get_required_json<'a>(inputs: &'a InputMap, key: &str) -> Result<&'a serde_json::Value, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_json().map_err(|_| type_err(key, SocketType::Json, v.type_of()))
}

/// M4: [`crate::motion::MotionFrame`] 入力（`motion_frame` ポート。`json` から coerce 済みでも可）。
pub fn get_required_motion_frame(inputs: &InputMap, key: &str) -> Result<crate::motion::MotionFrame, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_motion_frame()
		.map(|m| m.clone())
		.map_err(|_| type_err(key, SocketType::MotionFrame, v.type_of()))
}

pub fn get_required_list<'a>(inputs: &'a InputMap, key: &str) -> Result<&'a [SocketValue], NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_list()
		.map_err(|_| type_err(key, SocketType::List(Box::new(SocketType::Json)), v.type_of()))
}

pub fn get_required_map<'a>(inputs: &'a InputMap, key: &str) -> Result<&'a std::collections::BTreeMap<String, SocketValue>, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_map()
		.map_err(|_| type_err(key, SocketType::Map(Box::new(SocketType::Json)), v.type_of()))
}

pub fn get_optional_int(inputs: &InputMap, key: &str, default: i64) -> Result<i64, NodeExecError> {
	match inputs.get(key) {
		None => Ok(default),
		Some(v) => v.as_i64().map_err(|_| type_err(key, SocketType::Int, v.type_of())),
	}
}

pub fn get_optional_bool(inputs: &InputMap, key: &str, default: bool) -> Result<bool, NodeExecError> {
	match inputs.get(key) {
		None => Ok(default),
		Some(v) => v.as_bool().map_err(|_| type_err(key, SocketType::Bool, v.type_of())),
	}
}

pub fn get_optional_string(inputs: &InputMap, key: &str, default: &str) -> Result<String, NodeExecError> {
	match inputs.get(key) {
		None => Ok(default.to_owned()),
		Some(v) => v
			.as_str()
			.map(str::to_owned)
			.map_err(|_| type_err(key, SocketType::String, v.type_of())),
	}
}

pub fn get_optional_float(inputs: &InputMap, key: &str, default: f64) -> Result<f64, NodeExecError> {
	match inputs.get(key) {
		None => Ok(default),
		Some(v) => v.as_f64().map_err(|_| type_err(key, SocketType::Float, v.type_of())),
	}
}

/// Optional `Map<Json>` 読み取り。欠損時は空 BTreeMap を返す。
pub fn get_optional_map<'a>(
	inputs: &'a InputMap,
	key: &str,
) -> Result<std::borrow::Cow<'a, std::collections::BTreeMap<String, SocketValue>>, NodeExecError> {
	use std::borrow::Cow;
	match inputs.get(key) {
		None => Ok(Cow::Owned(std::collections::BTreeMap::new())),
		Some(v) => v
			.as_map()
			.map(Cow::Borrowed)
			.map_err(|_| type_err(key, SocketType::Map(Box::new(SocketType::Json)), v.type_of())),
	}
}

/// 実行時に「発火」した exec 入力/出力のリスト（port name, 順序保持・重複排除）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecFireSet {
	inner: Vec<String>,
}

impl ExecFireSet {
	pub fn new() -> Self {
		Self::default()
	}
	pub fn insert(&mut self, name: impl Into<String>) {
		let name = name.into();
		if !self.inner.iter().any(|e| e == &name) {
			self.inner.push(name);
		}
	}
	pub fn contains(&self, name: &str) -> bool {
		self.inner.iter().any(|e| e == name)
	}
	pub fn is_empty(&self) -> bool {
		self.inner.is_empty()
	}
	pub fn len(&self) -> usize {
		self.inner.len()
	}
	pub fn iter(&self) -> std::slice::Iter<'_, String> {
		self.inner.iter()
	}
	pub fn as_slice(&self) -> &[String] {
		&self.inner
	}
}

impl<'a> IntoIterator for &'a ExecFireSet {
	type Item = &'a String;
	type IntoIter = std::slice::Iter<'a, String>;
	fn into_iter(self) -> Self::IntoIter {
		self.iter()
	}
}

/// ノード実行の出力（data 出力 + どの exec 出力を発火させるか）。
#[derive(Debug, Clone, Default)]
pub struct NodeOutput {
	pub data: OutputMap,
	pub fired_exec: ExecFireSet,
}

impl NodeOutput {
	pub fn new() -> Self {
		Self::default()
	}
	pub fn set_data(mut self, port: &str, v: SocketValue) -> Self {
		self.data.insert(port.to_string(), v);
		self
	}
	pub fn fire_exec(mut self, port: &str) -> Self {
		self.fired_exec.insert(port);
		self
	}
}

/// 副作用ノードの実行文脈。I/O リソースはここ経由で供給する想定（δ-3 / δ-4c で拡張継続）。
///
/// - `trace`: Log ノード等が出力するトレース（テスト可視化用途）。
/// - `trigger`: `run_forever` 実行時のみ `Some`。EffectfulNode が自己 trigger や
///   他ノードの遅延発火を仕掛けたい場合に利用する。1-shot `execute()` では `None`。
/// - `node_id`: 実行中のノード ID。engine が呼び出し直前にセットする。
/// - `audio_sink`: アプリ全体で共有する音声再生シンク。TTS 系ノードが利用する。
///   headless テストや sink 未初期化の文脈では `None`。
/// - `state_handle`: `State`（`Arc<RwLock<State>>`）への weak 参照。
///   `channel.emit` ノード等が既存の ws push / Control API emit を叩くために使う。
///   1-shot `execute()` やテスト文脈では `None`。
#[derive(Debug, Default)]
pub struct ExecCtx {
	pub trace: Vec<String>,
	pub trigger: Option<TriggerHandle>,
	/// RM-3: 指定時、exec 経路（trigger / ソース / exec 連鎖）で非活性ノードは `fire_node` が即 return。
	pub trigger_gate: Option<std::sync::Arc<crate::flowgraph::activation::TriggerGate>>,
	pub node_id: String,
	pub audio_sink: Option<crate::SharedAudioSink>,
	pub state_handle: Option<std::sync::Weak<tokio::sync::RwLock<crate::state::State>>>,
	pub effect_mocks: Option<Arc<EffectMocks>>,
}

impl ExecCtx {
	pub fn log(&mut self, s: impl Into<String>) {
		self.trace.push(s.into());
	}
}

/// Fixture / debugger 用の副作用差し替え表。
///
/// 本番 runtime は `None` のまま動く。テスト時だけ node fq id を key にして、
/// 外部 I/O の代わりに決定論的な mock response を返す。
#[derive(Debug, Default, Clone)]
pub struct EffectMocks {
	pub http: HashMap<String, HttpMockResponse>,
}

impl EffectMocks {
	pub fn is_empty(&self) -> bool {
		self.http.is_empty()
	}

	pub fn http_response(&self, node_id: &str) -> Option<&HttpMockResponse> {
		self.http.get(node_id)
	}
}

#[derive(Debug, Clone)]
pub struct HttpMockResponse {
	pub status: i64,
	pub body_text: String,
	pub body_json: JsonValue,
	pub error: Option<String>,
}

/// StatefulNode 用の限定的な実行文脈。`ExecCtx` と違い I/O リソースには触れない。
/// 「self-ingress（自己 trigger）」能力のみを `trigger` で渡す。
pub struct StatefulCtx<'a> {
	pub node_id: &'a str,
	pub trigger: Option<&'a TriggerHandle>,
}

// ---------------------------------------------------------------------
// Trigger 基盤（spec §5.4）
// ---------------------------------------------------------------------

/// Engine の trigger queue に投入する 1 イベント。
///
/// - `node_id`: 発火させたいノード
/// - `fired_exec`: そのノードで「発火したものとして扱う」exec 入力ポート名の列
/// - `data_overrides`: 通常の pull を上書きするデータ。key はポート名、value はポート値。
///   主に Delay ノードが timer 完了時に「どの pending 値を取り出すか」を id として渡すのに使う。
#[derive(Debug, Clone, Default)]
pub struct TriggerEvent {
	pub node_id: String,
	pub fired_exec: Vec<String>,
	pub data_overrides: InputMap,
}

impl TriggerEvent {
	pub fn new(node_id: impl Into<String>) -> Self {
		Self {
			node_id: node_id.into(),
			fired_exec: Vec::new(),
			data_overrides: InputMap::new(),
		}
	}
	pub fn with_exec(mut self, port: impl Into<String>) -> Self {
		self.fired_exec.push(port.into());
		self
	}
	pub fn with_override(mut self, port: impl Into<String>, v: SocketValue) -> Self {
		self.data_overrides.insert(port.into(), v);
		self
	}
}

/// Trigger バス送信側のクローン可能ハンドル。
/// `tokio::spawn` したタスクや外部ハンドラがここから engine の event loop を叩く。
#[derive(Debug, Clone)]
pub struct TriggerHandle {
	sender: UnboundedSender<TriggerEvent>,
}

impl TriggerHandle {
	pub fn new(sender: UnboundedSender<TriggerEvent>) -> Self {
		Self { sender }
	}
	pub fn send(&self, event: TriggerEvent) -> Result<(), TriggerSendError> {
		self.sender.send(event).map_err(|_| TriggerSendError::ChannelClosed)
	}
	/// 即時発火（exec 指定のみ、data override なし）のショートカット。
	pub fn send_exec(&self, node_id: impl Into<String>, exec: impl Into<String>) -> Result<(), TriggerSendError> {
		self.send(TriggerEvent::new(node_id).with_exec(exec))
	}
}

#[derive(Debug, Error)]
pub enum TriggerSendError {
	#[error("trigger channel is closed")]
	ChannelClosed,
}

// ---------------------------------------------------------------------
// NodeDescriptor（静的メタデータ）
// ---------------------------------------------------------------------

pub trait NodeDescriptor: Send + Sync {
	fn describe(&self) -> NodeSpec;

	/// Phase φ-2: Control API (`POST /api/v1/control/flowgraph/{instance_id}/trigger/{node_id}`)
	/// からの外部トリガ発火を許可するかの opt-in フラグ。既定 `false`。
	///
	/// `NodeSpec` には持たせず trait 側に置くことで、既存の `NodeSpec { ... }` リテラル
	/// （各 describe() 実装）を触らずに dictionary.learn / dictionary.forget 等だけを
	/// opt-in にできる設計。
	///
	/// セキュリティ判定の source of truth は `NodeRegistry::is_control_triggerable()`（trait 経由）。
	/// GUI 向けの node-catalog API（φ-6）はレスポンス JSON に同じ trait 値を
	/// `control_triggerable` フィールドとして混ぜ込んで返すため、Flowgraph Editor は
	/// 仕様レベルでノードごとの発火可否を知ることができる。
	fn control_triggerable(&self) -> bool {
		false
	}
}

// ---------------------------------------------------------------------
// 3 種ノード trait（spec §3.4）
// ---------------------------------------------------------------------

/// Pure 評価に engine が供する、VAC プロセス上の**隻参照**（副作用 I/O ではない）。
#[derive(Debug, Default, Clone)]
pub struct PureEvalHost {
	/// `State.runtime_mode_id` と共有。未接続のテスト等では `None`。
	pub runtime_mode: Option<Arc<RwLock<Option<String>>>>,
	/// スロットが `None`（未上書き）のときの実効 mode id（`conf.default_runtime_mode`）。未設定なら空。
	pub default_runtime_mode: Option<String>,
}

impl PureEvalHost {
	/// 現在の Runtime Mode 文字列。未設定は `default_runtime_mode` または空。
	pub fn effective_runtime_mode_id(&self) -> String {
		let from_slot = self.runtime_mode.as_ref().and_then(|a| a.read().ok().and_then(|g| g.clone()));
		if let Some(s) = from_slot {
			return s;
		}
		self.default_runtime_mode.clone().unwrap_or_default()
	}
}

/// 純粋関数ノード。副作用なし・状態なし・決定論的。
///
/// - data 入出力のみ扱う（exec も発火可能、ただし副作用起因ではなく純粋決定）。
/// - `ExecCtx` を受け取らないため HTTP / ファイル等の I/O には直接触れない。ランタイム観測は `host` のみ。
/// - 出力は engine がメモ化し、同一 generation 内で複数回 pull されても 1 度しか評価されない。
/// - 例: Literal / StringConcat / JsonGet / Branch（条件で exec 出力を選ぶだけ）/ Sequence
#[async_trait]
pub trait PureNode: NodeDescriptor {
	async fn compute(
		&self,
		host: &PureEvalHost,
		props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError>;
}

/// 状態付きノード。engine が `Box<dyn Any + Send>` として保持する state slot を `&mut` で借りる。
/// 概念的には State モナド `(world, inputs) -> (world', outputs)` の 1 ステップ。
///
/// I/O はしない（`ExecCtx` を受け取らない）。内部状態の読み書きと、
/// `StatefulCtx::trigger` 経由の「自己 trigger」だけが許容される効果。
#[async_trait]
pub trait StatefulNode: NodeDescriptor {
	/// state の初期値を生成。engine が NodeInstance 作成時に 1 度だけ呼ぶ。
	fn init_state(&self) -> Box<dyn Any + Send>;

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
		ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError>;
}

/// 副作用ノード。I/O 可。必ず exec 発火経由で評価される。
///
/// data pull では決して起動しない。
#[async_trait]
pub trait EffectfulNode: NodeDescriptor {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError>;
}

// ---------------------------------------------------------------------
// NodeImpl enum（homogeneous コンテナ）
// ---------------------------------------------------------------------

/// engine 内部で各ノード実装を保持する enum。state slot は Stateful 時のみ使用。
pub enum NodeImpl {
	Pure(Arc<dyn PureNode>),
	Stateful {
		node: Arc<dyn StatefulNode>,
		state: Box<dyn Any + Send>,
	},
	Effectful(Arc<dyn EffectfulNode>),
}

impl NodeImpl {
	pub fn describe(&self) -> NodeSpec {
		match self {
			NodeImpl::Pure(n) => n.describe(),
			NodeImpl::Stateful { node, .. } => node.describe(),
			NodeImpl::Effectful(n) => n.describe(),
		}
	}

	pub fn pure(node: Arc<dyn PureNode>) -> Self {
		NodeImpl::Pure(node)
	}

	pub fn stateful(node: Arc<dyn StatefulNode>) -> Self {
		let state = node.init_state();
		NodeImpl::Stateful { node, state }
	}

	pub fn effectful(node: Arc<dyn EffectfulNode>) -> Self {
		NodeImpl::Effectful(node)
	}

	pub fn is_pure(&self) -> bool {
		matches!(self, NodeImpl::Pure(_))
	}
	pub fn is_stateful(&self) -> bool {
		matches!(self, NodeImpl::Stateful { .. })
	}
	pub fn is_effectful(&self) -> bool {
		matches!(self, NodeImpl::Effectful(_))
	}
}

impl std::fmt::Debug for NodeImpl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			NodeImpl::Pure(_) => f.write_str("NodeImpl::Pure"),
			NodeImpl::Stateful { .. } => f.write_str("NodeImpl::Stateful"),
			NodeImpl::Effectful(_) => f.write_str("NodeImpl::Effectful"),
		}
	}
}

// ---------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum NodeExecError {
	#[error("必須入力 '{0}' が未到達")]
	MissingRequiredInput(String),
	#[error("入力 '{port}' の型が不一致: expected {expected}, got {actual}")]
	TypeMismatch {
		port: String,
		expected: SocketType,
		actual: SocketType,
	},
	#[error("EffectfulNode '{0}' の出力を exec 発火前に pull しようとした")]
	EffectfulPulledBeforeFiring(String),
	#[error("ノード実行エラー: {0}")]
	Generic(#[from] anyhow::Error),
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn port_builders_work() {
		let input = PortSpec::input("cond", "Condition", SocketType::Bool);
		assert_eq!(input.direction, PortDirection::Input);
		assert!(!input.is_exec);
		let exec_out = PortSpec::exec_output("then", "Then");
		assert_eq!(exec_out.direction, PortDirection::Output);
		assert!(exec_out.is_exec);
		assert_eq!(exec_out.ty, SocketType::Exec);
	}

	#[test]
	fn node_spec_lookup() {
		let spec = NodeSpec {
			feature: "flowgraph.test".into(),
			title: "Test".into(),
			category: "test".into(),
			description: None,
			inputs: vec![PortSpec::input("a", "A", SocketType::Int)],
			outputs: vec![PortSpec::output("b", "B", SocketType::Int)],
			properties: vec![PropertySpec::new("p", "P", SocketType::String, SocketValue::String("x".into()))],
		};
		assert!(spec.find_input("a").is_some());
		assert!(spec.find_output("b").is_some());
		assert!(spec.find_property("p").is_some());
		assert!(spec.find_input("missing").is_none());
	}

	#[test]
	fn node_output_builders() {
		let out = NodeOutput::new().set_data("value", SocketValue::Int(42)).fire_exec("then");
		assert_eq!(out.data.get("value"), Some(&SocketValue::Int(42)));
		assert!(out.fired_exec.contains("then"));
	}

	#[test]
	fn property_spec_defaults() {
		let p = PropertySpec::new("host", "Host", SocketType::String, SocketValue::String("localhost".into()));
		assert_eq!(p.name, "host");
		assert_eq!(p.ty, SocketType::String);
		assert!(!p.required);
		let p = p.required();
		assert!(p.required);
	}

	/// ν-β-3 regression: Table 型 default の round-trip が engine 側で coerce できること。
	///
	/// `PortSpec::with_default(SocketValue::Table(Table::empty()))` は `SocketValueRepr` 経由で
	/// JSON `[]` として保存される。これを engine が pull 評価時に `def.to_socket_value(&port.ty)`
	/// で Table に戻すとき、空配列 + schema 未指定でも `Table::empty()` が得られる必要がある。
	/// ν-2.4 実装中に踏んだ `MissingRequiredInput("...:dictionary")` の根治パス。
	#[test]
	fn port_default_table_empty_round_trip() {
		use crate::flowgraph::table::Table;
		let port = PortSpec::input("dictionary", "Dictionary", SocketType::Table).with_default(SocketValue::Table(Table::empty()));
		let def = port.default.as_ref().expect("with_default must set default");
		let sv = def.to_socket_value(&port.ty).expect("empty Table default must coerce back");
		match sv {
			SocketValue::Table(t) => {
				assert_eq!(t.len(), 0, "coerced Table should be empty");
				assert_eq!(t.schema().len(), 0, "coerced Table should have empty schema");
			}
			other => panic!("expected SocketValue::Table, got {:?}", other),
		}
	}
}
