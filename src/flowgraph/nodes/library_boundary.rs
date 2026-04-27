//! Phase λ: ライブラリ境界の v0 スタブノード（将来の動的境界ポートの前置き）。
//!
//! 現状は単一 `string` チャネルのみ。動的な port 合成は phase doc の λ+ スコープ。

use crate::flowgraph::node::{
	ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PropertySpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

/// ライブラリの外部入力側（グラフ内では「値が出る」スタブ）。
pub struct LibraryInputNode;

impl NodeDescriptor for LibraryInputNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.library.input".into(),
			title: "Library Input".into(),
			category: "library".into(),
			description: Some("Phase λ v0: 単一 string 境界（プロパティ value）。将来は接続駆動の動的ポートを予定。".into()),
			inputs: vec![],
			outputs: vec![PortSpec::output("value", "Value", SocketType::String)],
			properties: vec![PropertySpec::new(
				"value",
				"Value",
				SocketType::String,
				SocketValue::String(String::new()),
			)],
		}
	}
}

#[async_trait]
impl PureNode for LibraryInputNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, properties: &InputMap, _inputs: &InputMap, _fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let v = properties
			.get("value")
			.cloned()
			.unwrap_or_else(|| SocketValue::String(String::new()));
		Ok(NodeOutput::new().set_data("value", v))
	}
}

/// ライブラリの外部出力側（グラフ内ではシンク）。
pub struct LibraryOutputNode;

impl NodeDescriptor for LibraryOutputNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.library.output".into(),
			title: "Library Output".into(),
			category: "library".into(),
			description: Some("Phase λ v0: 単一 string 境界（入力 value を受ける）。将来は外向き動的ポートを予定。".into()),
			inputs: vec![PortSpec::input("value", "Value", SocketType::String)],
			outputs: vec![],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for LibraryOutputNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _properties: &InputMap, inputs: &InputMap, _fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let _ = inputs.get("value");
		Ok(NodeOutput::new())
	}
}
