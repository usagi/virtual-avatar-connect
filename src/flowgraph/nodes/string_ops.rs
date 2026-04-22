//! 文字列操作ノード（全て PureNode）。
//!
//! - `string.concat`: `a` と `b` を連結
//! - `string.len`: 文字数（UTF-8 char 数）
//! - `string.contains`: 部分文字列を含むか
//! - `string.replace`: `pattern` を `replacement` に全置換（リテラル、正規表現ではない）
//! - `string.split`: `sep` で split して `List<String>` を返す
//! - `string.join`: `List<String>` を `sep` で join

use crate::flowgraph::node::{
 get_required_list, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput,
 NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

// ----- concat ---------------------------------------------------------------

pub struct StringConcatNode;
impl NodeDescriptor for StringConcatNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.string.concat".into(),
   title: "String Concat".into(),
   category: "string".into(),
   description: Some("a と b を連結".into()),
   inputs: vec![
    PortSpec::input("a", "A", SocketType::String),
    PortSpec::input("b", "B", SocketType::String),
   ],
   outputs: vec![PortSpec::output("result", "Result", SocketType::String)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringConcatNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let a = get_required_string(inputs, "a")?;
  let b = get_required_string(inputs, "b")?;
  Ok(NodeOutput::new().set_data("result", SocketValue::String(format!("{a}{b}"))))
 }
}

// ----- len ------------------------------------------------------------------

pub struct StringLenNode;
impl NodeDescriptor for StringLenNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.string.len".into(),
   title: "String Length".into(),
   category: "string".into(),
   description: Some("UTF-8 文字数（chars().count()）".into()),
   inputs: vec![PortSpec::input("s", "String", SocketType::String)],
   outputs: vec![PortSpec::output("len", "Length", SocketType::Int)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringLenNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let s = get_required_string(inputs, "s")?;
  Ok(NodeOutput::new().set_data("len", SocketValue::Int(s.chars().count() as i64)))
 }
}

// ----- contains -------------------------------------------------------------

pub struct StringContainsNode;
impl NodeDescriptor for StringContainsNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.string.contains".into(),
   title: "String Contains".into(),
   category: "string".into(),
   description: None,
   inputs: vec![
    PortSpec::input("haystack", "Haystack", SocketType::String),
    PortSpec::input("needle", "Needle", SocketType::String),
   ],
   outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringContainsNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let h = get_required_string(inputs, "haystack")?;
  let n = get_required_string(inputs, "needle")?;
  Ok(NodeOutput::new().set_data("result", SocketValue::Bool(h.contains(&n))))
 }
}

// ----- replace --------------------------------------------------------------

pub struct StringReplaceNode;
impl NodeDescriptor for StringReplaceNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.string.replace".into(),
   title: "String Replace".into(),
   category: "string".into(),
   description: Some("pattern (リテラル) を replacement に全置換".into()),
   inputs: vec![
    PortSpec::input("s", "String", SocketType::String),
    PortSpec::input("pattern", "Pattern", SocketType::String),
    PortSpec::input("replacement", "Replacement", SocketType::String),
   ],
   outputs: vec![PortSpec::output("result", "Result", SocketType::String)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringReplaceNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let s = get_required_string(inputs, "s")?;
  let p = get_required_string(inputs, "pattern")?;
  let r = get_required_string(inputs, "replacement")?;
  Ok(NodeOutput::new().set_data("result", SocketValue::String(s.replace(&p, &r))))
 }
}

// ----- split ----------------------------------------------------------------

pub struct StringSplitNode;
impl NodeDescriptor for StringSplitNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.string.split".into(),
   title: "String Split".into(),
   category: "string".into(),
   description: Some("sep で split して List<String> を返す".into()),
   inputs: vec![
    PortSpec::input("s", "String", SocketType::String),
    PortSpec::input("sep", "Separator", SocketType::String),
   ],
   outputs: vec![PortSpec::output("parts", "Parts", SocketType::List(Box::new(SocketType::String)))],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringSplitNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let s = get_required_string(inputs, "s")?;
  let sep = get_required_string(inputs, "sep")?;
  let parts: Vec<SocketValue> = if sep.is_empty() {
   s.chars().map(|c| SocketValue::String(c.to_string())).collect()
  } else {
   s.split(&sep).map(|p| SocketValue::String(p.to_string())).collect()
  };
  Ok(NodeOutput::new().set_data("parts", SocketValue::List(parts)))
 }
}

// ----- join -----------------------------------------------------------------

pub struct StringJoinNode;
impl NodeDescriptor for StringJoinNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.string.join".into(),
   title: "String Join".into(),
   category: "string".into(),
   description: Some("List<String> を sep で連結".into()),
   inputs: vec![
    PortSpec::input("parts", "Parts", SocketType::List(Box::new(SocketType::String))),
    PortSpec::input("sep", "Separator", SocketType::String),
   ],
   outputs: vec![PortSpec::output("result", "Result", SocketType::String)],
   properties: vec![],
  }
 }
}
#[async_trait]
impl PureNode for StringJoinNode {
 async fn compute(
  &self,
  _p: &InputMap,
  inputs: &InputMap,
  _fired: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let parts = get_required_list(inputs, "parts")?;
  let sep = get_required_string(inputs, "sep")?;
  let mut strs = Vec::with_capacity(parts.len());
  for (i, v) in parts.iter().enumerate() {
   match v.as_str() {
    Ok(s) => strs.push(s.to_string()),
    Err(_) => {
     return Err(NodeExecError::TypeMismatch {
      port: format!("parts[{i}]"),
      expected: SocketType::String,
      actual: v.type_of(),
     })
    }
   }
  }
  Ok(NodeOutput::new().set_data("result", SocketValue::String(strs.join(&sep))))
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 fn ss(k: &str, v: &str) -> (String, SocketValue) {
  (k.into(), SocketValue::String(v.into()))
 }

 #[tokio::test]
 async fn concat_basic() {
  let inputs: InputMap = [ss("a", "Hello, "), ss("b", "World!")].into_iter().collect();
  let out = StringConcatNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::String("Hello, World!".into())));
 }

 #[tokio::test]
 async fn len_counts_chars() {
  let inputs: InputMap = [ss("s", "あいう")].into_iter().collect();
  let out = StringLenNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("len"), Some(&SocketValue::Int(3)));
 }

 #[tokio::test]
 async fn contains_hits_and_misses() {
  let inputs: InputMap = [ss("haystack", "Hello, World!"), ss("needle", "World")].into_iter().collect();
  let out = StringContainsNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));

  let inputs: InputMap = [ss("haystack", "abc"), ss("needle", "xyz")].into_iter().collect();
  let out = StringContainsNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(false)));
 }

 #[tokio::test]
 async fn replace_all_occurrences() {
  let inputs: InputMap = [ss("s", "aaa"), ss("pattern", "a"), ss("replacement", "b")].into_iter().collect();
  let out = StringReplaceNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::String("bbb".into())));
 }

 #[tokio::test]
 async fn split_and_join_roundtrip() {
  let inputs: InputMap = [ss("s", "a,b,c"), ss("sep", ",")].into_iter().collect();
  let out = StringSplitNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  let parts = out.data.get("parts").cloned().unwrap();
  assert_eq!(
   parts,
   SocketValue::List(vec![
    SocketValue::String("a".into()),
    SocketValue::String("b".into()),
    SocketValue::String("c".into()),
   ])
  );

  let inputs: InputMap = [("parts".into(), parts), ("sep".into(), SocketValue::String("-".into()))].into_iter().collect();
  let out = StringJoinNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::String("a-b-c".into())));
 }

 #[tokio::test]
 async fn join_wrong_element_type_errors() {
  let parts = SocketValue::List(vec![SocketValue::String("a".into()), SocketValue::Int(2)]);
  let inputs: InputMap = [("parts".into(), parts), ("sep".into(), SocketValue::String(",".into()))].into_iter().collect();
  let e = StringJoinNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap_err();
  assert!(matches!(e, NodeExecError::TypeMismatch { .. }));
 }
}
