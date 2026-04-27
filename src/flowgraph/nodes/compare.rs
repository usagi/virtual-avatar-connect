//! 比較演算ノード（全て PureNode）。
//!
//! - `compare.eq` / `compare.neq`: 任意の SocketValue 同士（Bool/Int/Float/String/Json/List/Map 可）
//! - `compare.int_lt` / `compare.int_gt` / `compare.int_le` / `compare.int_ge`
//! - `compare.float_lt` / `compare.float_gt`

use crate::flowgraph::node::{
	get_required_float, get_required_int, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

// ----- eq / neq (Json universal) -------------------------------------------

/// Json 型の値同士を `==` で比較。
/// 他の型を比較したい場合は事前に `convert.to_json` で包む設計。
pub struct EqNode;
pub struct NeqNode;

impl NodeDescriptor for EqNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.compare.eq".into(),
			title: "Equal".into(),
			category: "compare".into(),
			description: Some("Json 値同士を比較".into()),
			inputs: vec![
				PortSpec::input("a", "A", SocketType::Json),
				PortSpec::input("b", "B", SocketType::Json),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for EqNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let a = crate::flowgraph::node::get_required_json(inputs, "a")?;
		let b = crate::flowgraph::node::get_required_json(inputs, "b")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Bool(a == b)))
	}
}

impl NodeDescriptor for NeqNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.compare.neq".into(),
			title: "Not Equal".into(),
			category: "compare".into(),
			description: None,
			inputs: vec![
				PortSpec::input("a", "A", SocketType::Json),
				PortSpec::input("b", "B", SocketType::Json),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for NeqNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let a = crate::flowgraph::node::get_required_json(inputs, "a")?;
		let b = crate::flowgraph::node::get_required_json(inputs, "b")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Bool(a != b)))
	}
}

// ----- int 比較 -------------------------------------------------------------

macro_rules! int_cmp_node {
 ($name:ident, $feature:literal, $title:literal, $op:tt) => {
  pub struct $name;
  impl NodeDescriptor for $name {
   fn describe(&self) -> NodeSpec {
    NodeSpec {
     feature: $feature.into(),
     title: $title.into(),
     category: "compare".into(),
     description: None,
     inputs: vec![
      PortSpec::input("a", "A", SocketType::Int),
      PortSpec::input("b", "B", SocketType::Int),
     ],
     outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
     properties: vec![],
    }
   }
  }
  #[async_trait]
  impl PureNode for $name {
   async fn compute(
    &self,
    _host: &crate::flowgraph::node::PureEvalHost,
    _p: &InputMap,
    inputs: &InputMap,
    _fired: &ExecFireSet,
   ) -> Result<NodeOutput, NodeExecError> {
    let a = get_required_int(inputs, "a")?;
    let b = get_required_int(inputs, "b")?;
    Ok(NodeOutput::new().set_data("result", SocketValue::Bool(a $op b)))
   }
  }
 };
}

int_cmp_node!(IntLtNode, "flowgraph.compare.int_lt", "Int <", <);
int_cmp_node!(IntGtNode, "flowgraph.compare.int_gt", "Int >", >);
int_cmp_node!(IntLeNode, "flowgraph.compare.int_le", "Int <=", <=);
int_cmp_node!(IntGeNode, "flowgraph.compare.int_ge", "Int >=", >=);

// ----- float 比較 -----------------------------------------------------------

macro_rules! float_cmp_node {
 ($name:ident, $feature:literal, $title:literal, $op:tt) => {
  pub struct $name;
  impl NodeDescriptor for $name {
   fn describe(&self) -> NodeSpec {
    NodeSpec {
     feature: $feature.into(),
     title: $title.into(),
     category: "compare".into(),
     description: None,
     inputs: vec![
      PortSpec::input("a", "A", SocketType::Float),
      PortSpec::input("b", "B", SocketType::Float),
     ],
     outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
     properties: vec![],
    }
   }
  }
  #[async_trait]
  impl PureNode for $name {
   async fn compute(
    &self,
    _host: &crate::flowgraph::node::PureEvalHost,
    _p: &InputMap,
    inputs: &InputMap,
    _fired: &ExecFireSet,
   ) -> Result<NodeOutput, NodeExecError> {
    let a = get_required_float(inputs, "a")?;
    let b = get_required_float(inputs, "b")?;
    Ok(NodeOutput::new().set_data("result", SocketValue::Bool(a $op b)))
   }
  }
 };
}

float_cmp_node!(FloatLtNode, "flowgraph.compare.float_lt", "Float <", <);
float_cmp_node!(FloatGtNode, "flowgraph.compare.float_gt", "Float >", >);

#[cfg(test)]
mod tests {
	use super::*;

	fn int_inputs(a: i64, b: i64) -> InputMap {
		[("a".into(), SocketValue::Int(a)), ("b".into(), SocketValue::Int(b))]
			.into_iter()
			.collect()
	}

	fn json_inputs(a: serde_json::Value, b: serde_json::Value) -> InputMap {
		[("a".into(), SocketValue::Json(a)), ("b".into(), SocketValue::Json(b))]
			.into_iter()
			.collect()
	}

	#[tokio::test]
	async fn eq_compares_json() {
		let n = EqNode;
		let out = n
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&json_inputs(serde_json::json!(1), serde_json::json!(1)),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));

		let out = n
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&json_inputs(serde_json::json!({"x": 1}), serde_json::json!({"x": 2})),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(false)));
	}

	#[tokio::test]
	async fn int_cmp_basic() {
		let out = IntLtNode
			.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &int_inputs(1, 2), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));

		let out = IntGtNode
			.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &int_inputs(5, 3), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));

		let out = IntLeNode
			.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &int_inputs(2, 2), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));

		let out = IntGeNode
			.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &int_inputs(2, 3), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(false)));
	}

	#[tokio::test]
	async fn float_cmp_basic() {
		let inputs: InputMap = [("a".into(), SocketValue::Float(1.5)), ("b".into(), SocketValue::Float(2.5))]
			.into_iter()
			.collect();
		let out = FloatLtNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));
	}
}
