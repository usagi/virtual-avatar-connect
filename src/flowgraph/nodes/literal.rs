//! 各型のリテラルソース。入力ポートを持たず、プロパティ `value` を出力ポート `value` にそのまま出す。
//!
//! PureNode 実装（副作用なし、決定論的）。FlowgraphProgram のソース集合に入る。

use crate::flowgraph::node::{
	ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PropertySpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

macro_rules! literal_node {
	($name:ident, $feature:literal, $title:literal, $ty:expr, $default:expr) => {
		pub struct $name;

		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "literal".into(),
					description: Some(format!("{} literal", stringify!($name))),
					inputs: vec![],
					outputs: vec![PortSpec::output("value", "Value", $ty)],
					properties: vec![PropertySpec::new("value", "Value", $ty, $default)],
				}
			}
		}

		#[async_trait]
		impl PureNode for $name {
			async fn compute(
				&self,
				_host: &crate::flowgraph::node::PureEvalHost,
				properties: &InputMap,
				_inputs: &InputMap,
				_fired_exec: &ExecFireSet,
			) -> Result<NodeOutput, NodeExecError> {
				let v = properties.get("value").cloned().unwrap_or_else(|| $default);
				Ok(NodeOutput::new().set_data("value", v))
			}
		}
	};
}

literal_node!(
	BoolLiteralNode,
	"flowgraph.literal.bool",
	"Bool Literal",
	SocketType::Bool,
	SocketValue::Bool(false)
);
literal_node!(
	IntLiteralNode,
	"flowgraph.literal.int",
	"Int Literal",
	SocketType::Int,
	SocketValue::Int(0)
);
literal_node!(
	FloatLiteralNode,
	"flowgraph.literal.float",
	"Float Literal",
	SocketType::Float,
	SocketValue::Float(0.0)
);
literal_node!(
	StringLiteralNode,
	"flowgraph.literal.string",
	"String Literal",
	SocketType::String,
	SocketValue::String(String::new())
);
literal_node!(
	JsonLiteralNode,
	"flowgraph.literal.json",
	"JSON Literal",
	SocketType::Json,
	SocketValue::Json(serde_json::Value::Null)
);

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn string_literal_emits_value() {
		let node = StringLiteralNode;
		let props: InputMap = [("value".to_string(), SocketValue::String("hi".into()))].into_iter().collect();
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&props,
				&InputMap::new(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("value"), Some(&SocketValue::String("hi".into())));
	}

	#[tokio::test]
	async fn int_literal_defaults_to_zero_on_missing_prop() {
		let node = IntLiteralNode;
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&InputMap::new(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("value"), Some(&SocketValue::Int(0)));
	}
}
