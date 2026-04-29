//! `flowgraph.bytes.*`: first-class bytes helpers (LF-5b).

use crate::flowgraph::node::{
	get_required_bytes, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use base64::Engine as _;

pub struct BytesFromBase64Node;

impl NodeDescriptor for BytesFromBase64Node {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.bytes.from_base64".into(),
			title: "Bytes From Base64".into(),
			category: "bytes".into(),
			description: Some("Base64 string を bytes にデコードする。失敗時はエラー halt。".into()),
			inputs: vec![PortSpec::input("text", "Text", SocketType::String)],
			outputs: vec![PortSpec::output("bytes", "Bytes", SocketType::Bytes)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for BytesFromBase64Node {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let text = get_required_string(inputs, "text")?;
		let bytes = base64::engine::general_purpose::STANDARD
			.decode(text.trim())
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("base64 decode error: {e}")))?;
		Ok(NodeOutput::new().set_data("bytes", SocketValue::Bytes(bytes)))
	}
}

pub struct BytesToBase64Node;

impl NodeDescriptor for BytesToBase64Node {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.bytes.to_base64".into(),
			title: "Bytes To Base64".into(),
			category: "bytes".into(),
			description: Some("bytes を Base64 string にエンコードする。".into()),
			inputs: vec![PortSpec::input("bytes", "Bytes", SocketType::Bytes)],
			outputs: vec![PortSpec::output("text", "Text", SocketType::String)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for BytesToBase64Node {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let bytes = get_required_bytes(inputs, "bytes")?;
		let text = base64::engine::general_purpose::STANDARD.encode(bytes);
		Ok(NodeOutput::new().set_data("text", SocketValue::String(text)))
	}
}

pub struct BytesLenNode;

impl NodeDescriptor for BytesLenNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.bytes.len".into(),
			title: "Bytes Length".into(),
			category: "bytes".into(),
			description: Some("bytes の byte length を返す。".into()),
			inputs: vec![PortSpec::input("bytes", "Bytes", SocketType::Bytes)],
			outputs: vec![PortSpec::output("len", "Length", SocketType::Int)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for BytesLenNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let bytes = get_required_bytes(inputs, "bytes")?;
		Ok(NodeOutput::new().set_data("len", SocketValue::Int(bytes.len() as i64)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn base64_roundtrip_and_len() {
		let inputs: InputMap = [("text".into(), SocketValue::String("AAH/".into()))].into_iter().collect();
		let decoded = BytesFromBase64Node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let bytes = decoded.data.get("bytes").cloned().unwrap();
		assert_eq!(bytes, SocketValue::Bytes(vec![0, 1, 255]));

		let inputs: InputMap = [("bytes".into(), bytes.clone())].into_iter().collect();
		let encoded = BytesToBase64Node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(encoded.data.get("text"), Some(&SocketValue::String("AAH/".into())));

		let inputs: InputMap = [("bytes".into(), bytes)].into_iter().collect();
		let len = BytesLenNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(len.data.get("len"), Some(&SocketValue::Int(3)));
	}

	#[tokio::test]
	async fn from_base64_invalid_errors() {
		let inputs: InputMap = [("text".into(), SocketValue::String("not base64!".into()))].into_iter().collect();
		let err = BytesFromBase64Node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(err, NodeExecError::Generic(_)));
	}
}
