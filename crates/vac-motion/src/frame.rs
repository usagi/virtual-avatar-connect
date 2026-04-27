//! Phase M4: VMC / OSC を正規化したワイヤ表現（**第一級 [`MotionFrame`]**）。
//!
//! Flowgraph では `SocketType::MotionFrame` として渡し、
//! `json` ポートとはエッジ上で暗黙変換（`byte_len` / `osc_messages` の JSON オブジェクト）可能。

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// OSC 1 メッセージのワイヤ抜き出し（JSON 互換の `args`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OscMessageWire {
	pub address: String,
	pub args: Vec<JsonValue>,
}

/// VMC ingress が届けた UDP ペイロードを OSC として解釈した結果（M4 ワイヤスキーマ）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionFrame {
	/// ワイヤ上の UDP バイト長。JSON からの復元で欠ける場合は `0`。
	#[serde(default)]
	pub byte_len: usize,
	#[serde(default)]
	pub osc_messages: Vec<OscMessageWire>,
}

impl MotionFrame {
	/// JSON 表現（`flowgraph.motion.*` の従来ワイヤと同一形状）。
	pub fn to_json_value(&self) -> JsonValue {
		serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
	}

	/// `address_prefix` / `address_substring`（いずれも空なら無条件）で `osc_messages` を絞り込む。`byte_len` は維持。
	pub fn filter_addresses(&self, prefix: &str, substring: &str) -> MotionFrame {
		let prefix = prefix.trim();
		let substring = substring.trim();
		let osc_messages: Vec<OscMessageWire> = self
			.osc_messages
			.iter()
			.filter(|m| {
				let addr = m.address.as_str();
				let pass_p = prefix.is_empty() || addr.starts_with(prefix);
				let pass_s = substring.is_empty() || addr.contains(substring);
				pass_p && pass_s
			})
			.cloned()
			.collect();
		MotionFrame {
			byte_len: self.byte_len,
			osc_messages,
		}
	}

	/// 各 `osc_messages[].args` 内の JSON 数値を再帰的に `scale` 倍する。`byte_len` は維持。
	pub fn map_numeric_args(&self, scale: f64) -> Result<MotionFrame, &'static str> {
		if !scale.is_finite() {
			return Err("scale は有限の数である必要があります");
		}
		let osc_messages = self
			.osc_messages
			.iter()
			.map(|m| OscMessageWire {
				address: m.address.clone(),
				args: m.args.iter().map(|a| scale_json_numbers(a, scale)).collect(),
			})
			.collect();
		Ok(MotionFrame {
			byte_len: self.byte_len,
			osc_messages,
		})
	}
}

fn scale_json_numbers(v: &JsonValue, scale: f64) -> JsonValue {
	match v {
		JsonValue::Number(n) => {
			let f = n.as_f64().unwrap_or(0.0) * scale;
			JsonValue::from(f)
		}
		JsonValue::Array(a) => JsonValue::Array(a.iter().map(|x| scale_json_numbers(x, scale)).collect()),
		JsonValue::Object(map) => JsonValue::Object(map.iter().map(|(k, val)| (k.clone(), scale_json_numbers(val, scale))).collect()),
		_ => v.clone(),
	}
}
