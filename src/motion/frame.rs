//! Phase M4: VMC / OSC を正規化した **JSON 表現**（第一歩）。
//!
//! Flowgraph では現状 [`crate::flowgraph::socket::SocketValue::Json`] として渡す。
//! 将来、第一級 `MotionFrame` 型や `flowgraph.motion.filter` 等へ繋ぐ際のスキーマ目安。

use serde::Serialize;

/// OSC 1 メッセージのワイヤ抜き出し（JSON 互換）。
#[derive(Debug, Clone, Serialize)]
pub struct OscMessageWire {
	pub address: String,
	pub args: Vec<serde_json::Value>,
}

/// VMC ingress が届けた UDP ペイロードを OSC として解釈した結果（M4 v0）。
#[derive(Debug, Clone, Serialize)]
pub struct MotionFrameV0 {
	pub byte_len: usize,
	pub osc_messages: Vec<OscMessageWire>,
}

impl MotionFrameV0 {
	pub fn to_json_value(&self) -> serde_json::Value {
		serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
	}
}
