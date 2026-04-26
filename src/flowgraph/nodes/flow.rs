//! 制御フロー系ノード（δ-1 最小版）。純粋関数 (PureNode) として実装。
//!
//! - `Branch`: `cond: Bool` で `then` / `else` の exec 出力を分岐発火
//! - `Sequence`: 入力ポートなし・プロパティなし。ソースとして働き、指定個の exec 出力を順に発火

use crate::flowgraph::node::{
	get_required_bool, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::SocketType;
use async_trait::async_trait;

// ----- Branch ---------------------------------------------------------------

pub struct BranchNode;

impl NodeDescriptor for BranchNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.flow.branch".into(),
			title: "Branch".into(),
			category: "flow".into(),
			description: Some("cond が true なら then を、false なら else を発火".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("cond", "Condition", SocketType::Bool),
			],
			outputs: vec![PortSpec::exec_output("then", "Then"), PortSpec::exec_output("else", "Else")],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for BranchNode {
	async fn compute(&self, _properties: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let cond = get_required_bool(inputs, "cond")?;
		Ok(if cond {
			NodeOutput::new().fire_exec("then")
		} else {
			NodeOutput::new().fire_exec("else")
		})
	}
}

// ----- Gate -----------------------------------------------------------------

/// `flow.gate`（ステートレス純関数版）: `open=true` の間 exec を通す。
/// open=false だと exec_in が来ても exec_out を発火しない。
///
/// UE Blueprint の `Gate` と違い、状態を持たない（Open/Close exec 入力はない）。
/// UE 互換が必要な場合は δ-3 で `state.bool` + `flow.gate` のサブグラフで表現する。
pub struct GateNode;

impl NodeDescriptor for GateNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.flow.gate".into(),
			title: "Gate".into(),
			category: "flow".into(),
			description: Some("open=true の間だけ exec を通す（ステートレス）".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("open", "Open", SocketType::Bool).with_default(crate::flowgraph::socket::SocketValue::Bool(true)),
			],
			outputs: vec![PortSpec::exec_output("exec_out", "Exec Out")],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for GateNode {
	async fn compute(&self, _properties: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let open = get_required_bool(inputs, "open")?;
		Ok(if open {
			NodeOutput::new().fire_exec("exec_out")
		} else {
			NodeOutput::new()
		})
	}
}

// ----- Sequence ------------------------------------------------------------

/// 指定個の exec 出力 (`exec_1`, `exec_2`, ...) を順に発火するソース。
///
/// 入力ポートなしのソースとして登録され、プログラム開始時に 1 回だけ呼ばれる。
/// 発火は `ExecFireSet` の順序保持により決定論的。
pub struct SequenceNode {
	count: usize,
}

impl SequenceNode {
	pub fn new(count: usize) -> Self {
		Self { count: count.max(1) }
	}
}

impl NodeDescriptor for SequenceNode {
	fn describe(&self) -> NodeSpec {
		let mut outputs = Vec::with_capacity(self.count);
		for i in 1..=self.count {
			outputs.push(PortSpec::exec_output(&format!("exec_{i}"), &format!("Exec {i}")));
		}
		NodeSpec {
			feature: "flowgraph.flow.sequence".into(),
			title: format!("Sequence ({})", self.count),
			category: "flow".into(),
			description: Some("プログラム開始時に exec_1..exec_N を順に発火するソース".into()),
			inputs: vec![],
			outputs,
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for SequenceNode {
	async fn compute(&self, _properties: &InputMap, _inputs: &InputMap, _fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let mut out = NodeOutput::new();
		for i in 1..=self.count {
			out = out.fire_exec(&format!("exec_{i}"));
		}
		Ok(out)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::socket::SocketValue;

	#[tokio::test]
	async fn branch_true_fires_then() {
		let node = BranchNode;
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [("cond".to_string(), SocketValue::Bool(true))].into_iter().collect();
		let out = node.compute(&InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("then"));
		assert!(!out.fired_exec.contains("else"));
	}

	#[tokio::test]
	async fn branch_false_fires_else() {
		let node = BranchNode;
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [("cond".to_string(), SocketValue::Bool(false))].into_iter().collect();
		let out = node.compute(&InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("else"));
		assert!(!out.fired_exec.contains("then"));
	}

	#[tokio::test]
	async fn sequence_fires_all_outputs() {
		let node = SequenceNode::new(3);
		let out = node.compute(&InputMap::new(), &InputMap::new(), &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.contains("exec_1"));
		assert!(out.fired_exec.contains("exec_2"));
		assert!(out.fired_exec.contains("exec_3"));
	}

	#[tokio::test]
	async fn gate_open_passes_exec() {
		let node = GateNode;
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [("open".into(), SocketValue::Bool(true))].into_iter().collect();
		let out = node.compute(&InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("exec_out"));
	}

	#[tokio::test]
	async fn gate_closed_blocks_exec() {
		let node = GateNode;
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [("open".into(), SocketValue::Bool(false))].into_iter().collect();
		let out = node.compute(&InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.is_empty());
	}
}
