//! 型変換ノード（全て PureNode）。
//!
//! 暗黙変換は spec §2.1 に従い行われないため、型が違うデータを繋ぐときは
//! 明示的にこれらのノードを挟む。

use crate::flowgraph::node::{
 get_required_float, get_required_int, get_required_string, ExecFireSet, InputMap, NodeDescriptor,
 NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

// ----- int → string --------------------------------------------------------

pub struct IntToStringNode;
impl NodeDescriptor for IntToStringNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.convert.int_to_string".into(),
   title: "Int → String".into(),
   category: "convert".into(),
   description: None,
   inputs: vec![PortSpec::input("value", "Value", SocketType::Int)],
   outputs: vec![PortSpec::output("result", "Result", SocketType::String)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for IntToStringNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let v = get_required_int(inputs, "value")?;
  Ok(NodeOutput::new().set_data("result", SocketValue::String(v.to_string())))
 }
}

// ----- string → int --------------------------------------------------------

pub struct StringToIntNode;
impl NodeDescriptor for StringToIntNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.convert.string_to_int".into(),
   title: "String → Int".into(),
   category: "convert".into(),
   description: Some("パース失敗はエラー halt".into()),
   inputs: vec![PortSpec::input("value", "Value", SocketType::String)],
   outputs: vec![PortSpec::output("result", "Result", SocketType::Int)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringToIntNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let s = get_required_string(inputs, "value")?;
  let v: i64 = s
   .trim()
   .parse()
   .map_err(|e| NodeExecError::Generic(anyhow::anyhow!("int parse error: {e} (input: {s:?})")))?;
  Ok(NodeOutput::new().set_data("result", SocketValue::Int(v)))
 }
}

// ----- float → string ------------------------------------------------------

pub struct FloatToStringNode;
impl NodeDescriptor for FloatToStringNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.convert.float_to_string".into(),
   title: "Float → String".into(),
   category: "convert".into(),
   description: None,
   inputs: vec![PortSpec::input("value", "Value", SocketType::Float)],
   outputs: vec![PortSpec::output("result", "Result", SocketType::String)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for FloatToStringNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let v = get_required_float(inputs, "value")?;
  Ok(NodeOutput::new().set_data("result", SocketValue::String(v.to_string())))
 }
}

// ----- string → float ------------------------------------------------------

pub struct StringToFloatNode;
impl NodeDescriptor for StringToFloatNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.convert.string_to_float".into(),
   title: "String → Float".into(),
   category: "convert".into(),
   description: Some("パース失敗はエラー halt".into()),
   inputs: vec![PortSpec::input("value", "Value", SocketType::String)],
   outputs: vec![PortSpec::output("result", "Result", SocketType::Float)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringToFloatNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let s = get_required_string(inputs, "value")?;
  let v: f64 = s
   .trim()
   .parse()
   .map_err(|e| NodeExecError::Generic(anyhow::anyhow!("float parse error: {e} (input: {s:?})")))?;
  Ok(NodeOutput::new().set_data("result", SocketValue::Float(v)))
 }
}

// ----- int → float ---------------------------------------------------------

pub struct IntToFloatNode;
impl NodeDescriptor for IntToFloatNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.convert.int_to_float".into(),
   title: "Int → Float".into(),
   category: "convert".into(),
   description: None,
   inputs: vec![PortSpec::input("value", "Value", SocketType::Int)],
   outputs: vec![PortSpec::output("result", "Result", SocketType::Float)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for IntToFloatNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let v = get_required_int(inputs, "value")?;
  Ok(NodeOutput::new().set_data("result", SocketValue::Float(v as f64)))
 }
}

// ----- float → int ---------------------------------------------------------

pub struct FloatToIntNode;
impl NodeDescriptor for FloatToIntNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.convert.float_to_int".into(),
   title: "Float → Int".into(),
   category: "convert".into(),
   description: Some("切り捨て（trunc）".into()),
   inputs: vec![PortSpec::input("value", "Value", SocketType::Float)],
   outputs: vec![PortSpec::output("result", "Result", SocketType::Int)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for FloatToIntNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let v = get_required_float(inputs, "value")?;
  Ok(NodeOutput::new().set_data("result", SocketValue::Int(v.trunc() as i64)))
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 async fn int_string_roundtrip() {
  let inputs: InputMap = [("value".into(), SocketValue::Int(42))].into_iter().collect();
  let out = IntToStringNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::String("42".into())));

  let inputs: InputMap = [("value".into(), SocketValue::String("42".into()))].into_iter().collect();
  let out = StringToIntNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Int(42)));
 }

 #[tokio::test]
 async fn string_to_int_fails_on_nonnumeric() {
  let inputs: InputMap = [("value".into(), SocketValue::String("abc".into()))].into_iter().collect();
  let e = StringToIntNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap_err();
  assert!(matches!(e, NodeExecError::Generic(_)));
 }

 #[tokio::test]
 async fn int_to_float_and_back() {
  let inputs: InputMap = [("value".into(), SocketValue::Int(3))].into_iter().collect();
  let out = IntToFloatNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Float(3.0)));

  let inputs: InputMap = [("value".into(), SocketValue::Float(3.7))].into_iter().collect();
  let out = FloatToIntNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Int(3)));
 }

 #[tokio::test]
 async fn float_string_roundtrip() {
  let inputs: InputMap = [("value".into(), SocketValue::Float(1.5))].into_iter().collect();
  let out = FloatToStringNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::String("1.5".into())));

  let inputs: InputMap = [("value".into(), SocketValue::String("1.5".into()))].into_iter().collect();
  let out = StringToFloatNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Float(1.5)));
 }
}
