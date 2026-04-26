//! Bool 論理演算ノード（全て PureNode）。

use crate::flowgraph::node::{
	get_required_bool, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

/// 二項論理演算マクロ（and / or / xor）。
macro_rules! binary_logic_node {
	($name:ident, $feature:literal, $title:literal, $op:expr) => {
		pub struct $name;

		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "logic".into(),
					description: None,
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Bool),
						PortSpec::input("b", "B", SocketType::Bool),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
					properties: vec![],
				}
			}
		}

		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let a = get_required_bool(inputs, "a")?;
				let b = get_required_bool(inputs, "b")?;
				let op: fn(bool, bool) -> bool = $op;
				Ok(NodeOutput::new().set_data("result", SocketValue::Bool(op(a, b))))
			}
		}
	};
}

binary_logic_node!(AndNode, "flowgraph.logic.and", "And", |a, b| a && b);
binary_logic_node!(OrNode, "flowgraph.logic.or", "Or", |a, b| a || b);
binary_logic_node!(XorNode, "flowgraph.logic.xor", "Xor", |a, b| a ^ b);

// ----- Not ------------------------------------------------------------------

pub struct NotNode;

impl NodeDescriptor for NotNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.logic.not".into(),
			title: "Not".into(),
			category: "logic".into(),
			description: None,
			inputs: vec![PortSpec::input("a", "A", SocketType::Bool)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for NotNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let a = get_required_bool(inputs, "a")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Bool(!a)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn bool_inputs(a: bool, b: bool) -> InputMap {
		[("a".into(), SocketValue::Bool(a)), ("b".into(), SocketValue::Bool(b))]
			.into_iter()
			.collect()
	}

	#[tokio::test]
	async fn and_truth_table() {
		let n = AndNode;
		for (a, b, ex) in [
			(true, true, true),
			(true, false, false),
			(false, true, false),
			(false, false, false),
		] {
			let out = n.compute(&InputMap::new(), &bool_inputs(a, b), &ExecFireSet::new()).await.unwrap();
			assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(ex)));
		}
	}

	#[tokio::test]
	async fn or_truth_table() {
		let n = OrNode;
		for (a, b, ex) in [(true, true, true), (true, false, true), (false, true, true), (false, false, false)] {
			let out = n.compute(&InputMap::new(), &bool_inputs(a, b), &ExecFireSet::new()).await.unwrap();
			assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(ex)));
		}
	}

	#[tokio::test]
	async fn xor_truth_table() {
		let n = XorNode;
		for (a, b, ex) in [(true, true, false), (true, false, true), (false, true, true), (false, false, false)] {
			let out = n.compute(&InputMap::new(), &bool_inputs(a, b), &ExecFireSet::new()).await.unwrap();
			assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(ex)));
		}
	}

	#[tokio::test]
	async fn not_truth_table() {
		let n = NotNode;
		for (a, ex) in [(true, false), (false, true)] {
			let inputs: InputMap = [("a".into(), SocketValue::Bool(a))].into_iter().collect();
			let out = n.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
			assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(ex)));
		}
	}

	#[tokio::test]
	async fn type_mismatch_errors() {
		let n = AndNode;
		let inputs: InputMap = [("a".into(), SocketValue::Int(1)), ("b".into(), SocketValue::Bool(true))]
			.into_iter()
			.collect();
		let e = n.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap_err();
		assert!(matches!(e, NodeExecError::TypeMismatch { .. }));
	}
}
