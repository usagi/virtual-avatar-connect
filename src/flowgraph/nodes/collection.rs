//! List / Map 操作ノード（全て PureNode）。
//!
//! Flowgraph の List は要素型が型パラメータ付きだが、δ-2 の組込みノードは
//! `List<Json>` / `Map<Json>` を前提とする（汎用性重視）。
//! 特定型のリストが欲しい場合は利用者側で `convert` ノードを挟む。

use crate::flowgraph::node::{
	get_required_int, get_required_list, get_required_map, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError,
	NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

// ----- list.len -------------------------------------------------------------

pub struct ListLenNode;
impl NodeDescriptor for ListLenNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.list.len".into(),
			title: "List Length".into(),
			category: "list".into(),
			description: None,
			inputs: vec![PortSpec::input("items", "Items", SocketType::List(Box::new(SocketType::Json)))],
			outputs: vec![PortSpec::output("len", "Length", SocketType::Int)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for ListLenNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let xs = get_required_list(inputs, "items")?;
		Ok(NodeOutput::new().set_data("len", SocketValue::Int(xs.len() as i64)))
	}
}

// ----- list.get -------------------------------------------------------------

pub struct ListGetNode;
impl NodeDescriptor for ListGetNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.list.get".into(),
			title: "List Get".into(),
			category: "list".into(),
			description: Some("index が範囲外なら Json::Null".into()),
			inputs: vec![
				PortSpec::input("items", "Items", SocketType::List(Box::new(SocketType::Json))),
				PortSpec::input("index", "Index", SocketType::Int),
			],
			outputs: vec![PortSpec::output("item", "Item", SocketType::Json)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for ListGetNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let xs = get_required_list(inputs, "items")?;
		let idx = get_required_int(inputs, "index")?;
		let v = if idx < 0 || idx as usize >= xs.len() {
			SocketValue::Json(serde_json::Value::Null)
		} else {
			match &xs[idx as usize] {
				SocketValue::Json(j) => SocketValue::Json(j.clone()),
				other => SocketValue::Json(socket_value_to_json(other)),
			}
		};
		Ok(NodeOutput::new().set_data("item", v))
	}
}

// ----- list.is_empty --------------------------------------------------------

pub struct ListIsEmptyNode;
impl NodeDescriptor for ListIsEmptyNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.list.is_empty".into(),
			title: "List Is Empty".into(),
			category: "list".into(),
			description: None,
			inputs: vec![PortSpec::input("items", "Items", SocketType::List(Box::new(SocketType::Json)))],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for ListIsEmptyNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let xs = get_required_list(inputs, "items")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Bool(xs.is_empty())))
	}
}

// ----- map.get --------------------------------------------------------------

pub struct MapGetNode;
impl NodeDescriptor for MapGetNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.map.get".into(),
			title: "Map Get".into(),
			category: "map".into(),
			description: Some("キーが存在しない場合は Json::Null".into()),
			inputs: vec![
				PortSpec::input("m", "Map", SocketType::Map(Box::new(SocketType::Json))),
				PortSpec::input("key", "Key", SocketType::String),
			],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Json)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for MapGetNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let m = get_required_map(inputs, "m")?;
		let k = get_required_string(inputs, "key")?;
		let v = match m.get(&k) {
			Some(SocketValue::Json(j)) => SocketValue::Json(j.clone()),
			Some(other) => SocketValue::Json(socket_value_to_json(other)),
			None => SocketValue::Json(serde_json::Value::Null),
		};
		Ok(NodeOutput::new().set_data("value", v))
	}
}

// ----- map.keys -------------------------------------------------------------

pub struct MapKeysNode;
impl NodeDescriptor for MapKeysNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.map.keys".into(),
			title: "Map Keys".into(),
			category: "map".into(),
			description: None,
			inputs: vec![PortSpec::input("m", "Map", SocketType::Map(Box::new(SocketType::Json)))],
			outputs: vec![PortSpec::output("keys", "Keys", SocketType::List(Box::new(SocketType::String)))],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for MapKeysNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let m = get_required_map(inputs, "m")?;
		let keys: Vec<SocketValue> = m.keys().cloned().map(SocketValue::String).collect();
		Ok(NodeOutput::new().set_data("keys", SocketValue::List(keys)))
	}
}

// ----- map.has --------------------------------------------------------------

pub struct MapHasNode;
impl NodeDescriptor for MapHasNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.map.has".into(),
			title: "Map Has".into(),
			category: "map".into(),
			description: None,
			inputs: vec![
				PortSpec::input("m", "Map", SocketType::Map(Box::new(SocketType::Json))),
				PortSpec::input("key", "Key", SocketType::String),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for MapHasNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let m = get_required_map(inputs, "m")?;
		let k = get_required_string(inputs, "key")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Bool(m.contains_key(&k))))
	}
}

// ----- 内部ヘルパ ----------------------------------------------------------

fn socket_value_to_json(v: &SocketValue) -> serde_json::Value {
	match v {
		SocketValue::Bool(b) => serde_json::json!(b),
		SocketValue::Int(i) => serde_json::json!(i),
		SocketValue::Float(f) => serde_json::json!(f),
		SocketValue::String(s) => serde_json::json!(s),
		SocketValue::Json(j) => j.clone(),
		SocketValue::List(xs) => serde_json::Value::Array(xs.iter().map(socket_value_to_json).collect()),
		SocketValue::Map(m) => {
			let obj: serde_json::Map<String, serde_json::Value> = m.iter().map(|(k, v)| (k.clone(), socket_value_to_json(v))).collect();
			serde_json::Value::Object(obj)
		}
		SocketValue::Table(t) => t.to_json_array(),
		// Phase ξ §6.4: 外部 JSON 境界では value のみ（pass-through）。
		// unit を維持したい場合は `flowgraph.unit.to_json` を使う。
		SocketValue::Quantity(q) => serde_json::json!(q.value),
		// Phase π: DateTime は RFC3339 (Z suffix) 文字列として pass-through。
		SocketValue::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn json_list(items: Vec<serde_json::Value>) -> SocketValue {
		SocketValue::List(items.into_iter().map(SocketValue::Json).collect())
	}

	fn json_map(entries: Vec<(&str, serde_json::Value)>) -> SocketValue {
		let mut m = std::collections::BTreeMap::new();
		for (k, v) in entries {
			m.insert(k.to_string(), SocketValue::Json(v));
		}
		SocketValue::Map(m)
	}

	#[tokio::test]
	async fn list_ops() {
		let items = json_list(vec![serde_json::json!(10), serde_json::json!(20), serde_json::json!(30)]);
		let inputs: InputMap = [("items".into(), items.clone())].into_iter().collect();
		let out = ListLenNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("len"), Some(&SocketValue::Int(3)));

		let inputs: InputMap = [("items".into(), items.clone()), ("index".into(), SocketValue::Int(1))]
			.into_iter()
			.collect();
		let out = ListGetNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("item"), Some(&SocketValue::Json(serde_json::json!(20))));

		let inputs: InputMap = [("items".into(), items), ("index".into(), SocketValue::Int(99))]
			.into_iter()
			.collect();
		let out = ListGetNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("item"), Some(&SocketValue::Json(serde_json::Value::Null)));

		let inputs: InputMap = [("items".into(), json_list(vec![]))].into_iter().collect();
		let out = ListIsEmptyNode
			.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));
	}

	#[tokio::test]
	async fn map_ops() {
		let m = json_map(vec![("a", serde_json::json!(1)), ("b", serde_json::json!("hi"))]);
		let inputs: InputMap = [("m".into(), m.clone()), ("key".into(), SocketValue::String("a".into()))]
			.into_iter()
			.collect();
		let out = MapGetNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("value"), Some(&SocketValue::Json(serde_json::json!(1))));

		let inputs: InputMap = [("m".into(), m.clone()), ("key".into(), SocketValue::String("missing".into()))]
			.into_iter()
			.collect();
		let out = MapGetNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("value"), Some(&SocketValue::Json(serde_json::Value::Null)));

		let inputs: InputMap = [("m".into(), m.clone())].into_iter().collect();
		let out = MapKeysNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(
			out.data.get("keys"),
			Some(&SocketValue::List(vec![
				SocketValue::String("a".into()),
				SocketValue::String("b".into())
			]))
		);

		let inputs: InputMap = [("m".into(), m), ("key".into(), SocketValue::String("b".into()))]
			.into_iter()
			.collect();
		let out = MapHasNode.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));
	}
}
