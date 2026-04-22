//! Math nodes (all PureNode).
//!
//! `int_*` and `float_*` are split by type. No implicit conversion (spec 2.1).
//! Division by zero halts with `NodeExecError::Generic` (delta-0 safety-first policy).

use crate::flowgraph::node::{
 get_required_float, get_required_int, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec,
 PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

// ---- Int binary ops --------------------------------------------------------

macro_rules! int_binop_node {
 ($name:ident, $feature:literal, $title:literal, $fn:expr) => {
  pub struct $name;
  impl NodeDescriptor for $name {
   fn describe(&self) -> NodeSpec {
    NodeSpec {
     feature: $feature.into(),
     title: $title.into(),
     category: "math".into(),
     description: None,
     inputs: vec![
      PortSpec::input("a", "A", SocketType::Int),
      PortSpec::input("b", "B", SocketType::Int),
     ],
     outputs: vec![PortSpec::output("result", "Result", SocketType::Int)],
     properties: vec![],
    }
   }
  }
  #[async_trait]
  impl PureNode for $name {
   async fn compute(
    &self,
    _p: &InputMap,
    inputs: &InputMap,
    _fired: &ExecFireSet,
   ) -> Result<NodeOutput, NodeExecError> {
    let a = get_required_int(inputs, "a")?;
    let b = get_required_int(inputs, "b")?;
    let op: fn(i64, i64) -> Result<i64, anyhow::Error> = $fn;
    let r = op(a, b)?;
    Ok(NodeOutput::new().set_data("result", SocketValue::Int(r)))
   }
  }
 };
}

int_binop_node!(IntAddNode, "flowgraph.math.int_add", "Int +", |a, b| Ok(a.wrapping_add(b)));
int_binop_node!(IntSubNode, "flowgraph.math.int_sub", "Int -", |a, b| Ok(a.wrapping_sub(b)));
int_binop_node!(IntMulNode, "flowgraph.math.int_mul", "Int *", |a, b| Ok(a.wrapping_mul(b)));
int_binop_node!(IntDivNode, "flowgraph.math.int_div", "Int /", |a, b| {
 if b == 0 {
  Err(anyhow::anyhow!("division by zero: a / b where b = 0"))
 } else {
  Ok(a.wrapping_div(b))
 }
});
int_binop_node!(IntModNode, "flowgraph.math.int_mod", "Int %", |a, b| {
 if b == 0 {
  Err(anyhow::anyhow!("modulo by zero: a % b where b = 0"))
 } else {
  Ok(a.wrapping_rem(b))
 }
});

// ---- Float binary ops ------------------------------------------------------

macro_rules! float_binop_node {
 ($name:ident, $feature:literal, $title:literal, $fn:expr) => {
  pub struct $name;
  impl NodeDescriptor for $name {
   fn describe(&self) -> NodeSpec {
    NodeSpec {
     feature: $feature.into(),
     title: $title.into(),
     category: "math".into(),
     description: None,
     inputs: vec![
      PortSpec::input("a", "A", SocketType::Float),
      PortSpec::input("b", "B", SocketType::Float),
     ],
     outputs: vec![PortSpec::output("result", "Result", SocketType::Float)],
     properties: vec![],
    }
   }
  }
  #[async_trait]
  impl PureNode for $name {
   async fn compute(
    &self,
    _p: &InputMap,
    inputs: &InputMap,
    _fired: &ExecFireSet,
   ) -> Result<NodeOutput, NodeExecError> {
    let a = get_required_float(inputs, "a")?;
    let b = get_required_float(inputs, "b")?;
    let op: fn(f64, f64) -> f64 = $fn;
    Ok(NodeOutput::new().set_data("result", SocketValue::Float(op(a, b))))
   }
  }
 };
}

float_binop_node!(FloatAddNode, "flowgraph.math.float_add", "Float +", |a, b| a + b);
float_binop_node!(FloatSubNode, "flowgraph.math.float_sub", "Float -", |a, b| a - b);
float_binop_node!(FloatMulNode, "flowgraph.math.float_mul", "Float *", |a, b| a * b);
float_binop_node!(FloatDivNode, "flowgraph.math.float_div", "Float /", |a, b| a / b);

#[cfg(test)]
mod tests {
 use super::*;

 fn int_in(a: i64, b: i64) -> InputMap {
  [("a".into(), SocketValue::Int(a)), ("b".into(), SocketValue::Int(b))].into_iter().collect()
 }

 #[tokio::test]
 async fn int_arithmetic() {
  let out = IntAddNode.compute(&InputMap::new(), &int_in(3, 4), &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Int(7)));

  let out = IntSubNode.compute(&InputMap::new(), &int_in(10, 3), &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Int(7)));

  let out = IntMulNode.compute(&InputMap::new(), &int_in(6, 7), &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Int(42)));

  let out = IntDivNode.compute(&InputMap::new(), &int_in(20, 4), &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Int(5)));

  let out = IntModNode.compute(&InputMap::new(), &int_in(17, 5), &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Int(2)));
 }

 #[tokio::test]
 async fn int_div_by_zero_errors() {
  let e = IntDivNode.compute(&InputMap::new(), &int_in(1, 0), &ExecFireSet::new()).await.unwrap_err();
  assert!(matches!(e, NodeExecError::Generic(_)));
 }

 #[tokio::test]
 async fn float_arithmetic() {
  let inputs: InputMap =
   [("a".into(), SocketValue::Float(1.5)), ("b".into(), SocketValue::Float(2.5))].into_iter().collect();
  let out = FloatAddNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Float(4.0)));

  let out = FloatMulNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Float(3.75)));
 }
}
