//! Math nodes (all PureNode).
//!
//! `int_*` and `float_*` are split by type. No implicit conversion (spec 2.1).
//! Division by zero halts with `NodeExecError::Generic` (delta-0 safety-first policy).
//!
//! Phase ξ-3: `float_*` ノードは `SocketType::Quantity` 入出力に移行した。
//! 既存の `Float` エッジは engine 側（[`crate::flowgraph::socket::coerce_to_type`]）で
//! dimensionless Quantity へ暗黙 wrap されるため、後方互換性は維持される。

use crate::flowgraph::node::{
 get_required_int, get_required_quantity, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec,
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

// ---- Float binary ops (Phase xi-3: Quantity-based) ------------------------

/// `flowgraph.math.float_*` 系の共通実装。Quantity を入出力として次元計算を行う。
///
/// `$op` は `Quantity::try_add` 等の関数ポインタ。dim 不一致や
/// K vs `\u{0394}`K のようなルール違反は `QuantityArithError` で返り、
/// engine には `NodeExecError::Generic` として伝播する。
macro_rules! float_binop_node {
 ($name:ident, $feature:literal, $title:literal, $desc:literal, $op:expr) => {
  pub struct $name;
  impl NodeDescriptor for $name {
   fn describe(&self) -> NodeSpec {
    NodeSpec {
     feature: $feature.into(),
     title: $title.into(),
     category: "math".into(),
     description: Some($desc.into()),
     inputs: vec![
      PortSpec::input("a", "A", SocketType::Quantity),
      PortSpec::input("b", "B", SocketType::Quantity),
     ],
     outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
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
    let a = get_required_quantity(inputs, "a")?;
    let b = get_required_quantity(inputs, "b")?;
    let op: fn(
     &crate::flowgraph::quantity::Quantity,
     &crate::flowgraph::quantity::Quantity,
    ) -> Result<
     crate::flowgraph::quantity::Quantity,
     crate::flowgraph::quantity::QuantityArithError,
    > = $op;
    let r = op(a, b).map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?;
    Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
   }
  }
 };
}

float_binop_node!(
 FloatAddNode,
 "flowgraph.math.float_add",
 "Float +",
 "Quantity 加算。dim 不一致はエラー。\u{0394}K + K(abs) は許容、K + K はエラー（abs 同士加算禁止）。",
 |a, b| a.try_add(b)
);
float_binop_node!(
 FloatSubNode,
 "flowgraph.math.float_sub",
 "Float -",
 "Quantity 減算。dim 不一致はエラー。K - K は \u{0394}K を生成。",
 |a, b| a.try_sub(b)
);
float_binop_node!(
 FloatMulNode,
 "flowgraph.math.float_mul",
 "Float *",
 "Quantity 乗算。dim は組み立てられる（m * s = m\u{00B7}s）。絶対温度を絡めた乗算は禁止。",
 |a, b| a.try_mul(b)
);
float_binop_node!(
 FloatDivNode,
 "flowgraph.math.float_div",
 "Float /",
 "Quantity 除算。dim は差分で組み立てられる（m / s = m\u{00B7}s\u{207B}\u{00B9}）。絶対温度の絡む除算や 0 除算はエラー。",
 |a, b| {
  if b.value == 0.0 {
   return Err(crate::flowgraph::quantity::QuantityArithError::DivisionByZero);
  }
  a.try_div(b)
 }
);

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
  use crate::flowgraph::quantity::Quantity;
  let inputs: InputMap = [
   ("a".into(), SocketValue::Quantity(Quantity::dimensionless(1.5))),
   ("b".into(), SocketValue::Quantity(Quantity::dimensionless(2.5))),
  ]
  .into_iter()
  .collect();
  let out = FloatAddNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  let Some(SocketValue::Quantity(q)) = out.data.get("result") else {
   panic!("expected Quantity result");
  };
  assert!((q.value - 4.0).abs() < 1e-12);
  assert!(q.is_dimensionless());

  let out = FloatMulNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  let Some(SocketValue::Quantity(q)) = out.data.get("result") else {
   panic!("expected Quantity result");
  };
  assert!((q.value - 3.75).abs() < 1e-12);
 }

 #[tokio::test]
 async fn float_add_with_units_dim_mismatch_errors() {
  use crate::flowgraph::quantity::{parse_unit, Quantity};
  let m = Quantity::of(1.0, parse_unit("m").unwrap());
  let s = Quantity::of(2.0, parse_unit("s").unwrap());
  let inputs: InputMap =
   [("a".into(), SocketValue::Quantity(m)), ("b".into(), SocketValue::Quantity(s))].into_iter().collect();
  let e = FloatAddNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap_err();
  assert!(matches!(e, NodeExecError::Generic(_)), "got {e:?}");
 }

 #[tokio::test]
 async fn float_mul_composes_units() {
  use crate::flowgraph::quantity::{parse_unit, Quantity};
  let m = Quantity::of(3.0, parse_unit("m").unwrap());
  let s = Quantity::of(4.0, parse_unit("s").unwrap());
  let inputs: InputMap =
   [("a".into(), SocketValue::Quantity(m)), ("b".into(), SocketValue::Quantity(s))].into_iter().collect();
  let out = FloatMulNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  let Some(SocketValue::Quantity(q)) = out.data.get("result") else {
   panic!("expected Quantity result");
  };
  assert!((q.value - 12.0).abs() < 1e-12);
  assert_eq!(q.unit.canonical(), "m\u{00B7}s");
 }

 #[tokio::test]
 async fn float_div_by_zero_errors() {
  use crate::flowgraph::quantity::Quantity;
  let inputs: InputMap = [
   ("a".into(), SocketValue::Quantity(Quantity::dimensionless(1.0))),
   ("b".into(), SocketValue::Quantity(Quantity::dimensionless(0.0))),
  ]
  .into_iter()
  .collect();
  let e = FloatDivNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap_err();
  assert!(matches!(e, NodeExecError::Generic(_)));
 }
}
