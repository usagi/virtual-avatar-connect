//! 副作用ノード `Log`: `value` 入力を `ctx.trace` に書き出す。
//!
//! - 入力: `exec_in` (Exec), `value` (String)
//! - 出力: `exec_out` (Exec)
//! - 発火契機: `exec_in`
//!
//! Phase \u{3be}-4 以降: `value` ポートは `String` のままだが、engine レベルの暗黙
//! coerce ([`crate::flowgraph::socket::coerce_to_type`]) により `Quantity` を
//! そのまま配線できる。配信される文字列は [`crate::flowgraph::quantity::Quantity`]
//! の `Display` 実装による `"{value} {unit}"` 形式（dimensionless なら `"{value}"`
//! のみ）。単位を外したい / 精度を制御したい場合は `flowgraph.util.format` を
//! 手前に挟む。

use crate::flowgraph::node::{
	EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::SocketType;
use async_trait::async_trait;

pub struct LogNode;

impl NodeDescriptor for LogNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.log".into(),
			title: "Log".into(),
			category: "util".into(),
			description: Some(
				"value 入力を trace に書き出し exec_out を発火。`Quantity` を流した場合は \
     engine 側で `\"{value} {unit}\"` 形式に自動文字列化される（dimensionless は数値のみ）。\
     精度や単位の ON/OFF を制御したい場合は `flowgraph.util.format` を挟む。"
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("value", "Value", SocketType::String),
			],
			outputs: vec![PortSpec::exec_output("exec_out", "Exec Out")],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for LogNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_properties: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let msg = inputs.get("value").and_then(|v| v.as_str().ok()).unwrap_or("").to_string();
		ctx.log(format!("log: {msg}"));
		Ok(NodeOutput::new().fire_exec("exec_out"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::socket::SocketValue;

	#[tokio::test]
	async fn log_writes_to_trace_on_exec() {
		let node = LogNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [("value".to_string(), SocketValue::String("hello".into()))].into_iter().collect();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
		assert_eq!(ctx.trace.len(), 1);
		assert!(ctx.trace[0].contains("hello"));
		assert!(out.fired_exec.contains("exec_out"));
	}

	#[tokio::test]
	async fn log_does_nothing_without_exec_firing() {
		let node = LogNode;
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [("value".to_string(), SocketValue::String("nope".into()))].into_iter().collect();
		let out = node
			.execute(&mut ctx, &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!(ctx.trace.is_empty());
		assert!(out.fired_exec.is_empty());
	}
}
