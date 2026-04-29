//! JSON 操作ノード（全て PureNode）。
//!
//! - `json.parse`: String → Json
//! - `json.stringify`: Json → String（`pretty` 省略時は compact）
//! - `json.get`: ドットパス（`foo.bar[0].baz`）で Json を抜き出す

use crate::flowgraph::node::{
	get_optional_bool, get_required_json, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec,
	PortSpec, PureNode,
};
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use async_trait::async_trait;

// ----- parse ----------------------------------------------------------------

pub struct JsonParseNode;
impl NodeDescriptor for JsonParseNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.json.parse".into(),
			title: "JSON Parse".into(),
			category: "json".into(),
			description: Some("String を JSON にパース（失敗時はエラー halt）".into()),
			inputs: vec![PortSpec::input("text", "Text", SocketType::String)],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Json)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for JsonParseNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let t = get_required_string(inputs, "text")?;
		let v: serde_json::Value =
			serde_json::from_str(&t).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("JSON parse error: {e}")))?;
		Ok(NodeOutput::new().set_data("value", SocketValue::Json(v)))
	}
}

pub struct JsonTryParseNode;
impl NodeDescriptor for JsonTryParseNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.json.try_parse".into(),
			title: "JSON Try Parse".into(),
			category: "json".into(),
			description: Some("String を JSON にパースし、失敗を result<json> として返す。".into()),
			inputs: vec![PortSpec::input("text", "Text", SocketType::String)],
			outputs: vec![
				PortSpec::output("ok", "OK", SocketType::Bool),
				PortSpec::output("value", "Value", SocketType::Json),
				PortSpec::output("error", "Error", SocketType::String),
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Json))),
			],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for JsonTryParseNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let text = get_required_string(inputs, "text")?;
		match serde_json::from_str::<serde_json::Value>(&text) {
			Ok(value) => Ok(NodeOutput::new()
				.set_data("ok", SocketValue::Bool(true))
				.set_data("value", SocketValue::Json(value.clone()))
				.set_data("error", SocketValue::String(String::new()))
				.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Json(value))))),
			Err(e) => {
				let error = format!("JSON parse error: {e}");
				Ok(NodeOutput::new()
					.set_data("ok", SocketValue::Bool(false))
					.set_data("value", SocketValue::Json(serde_json::Value::Null))
					.set_data("error", SocketValue::String(error.clone()))
					.set_data("result", SocketValue::Result(FlowResult::err(error).with_code("json.parse"))))
			}
		}
	}
}

// ----- stringify ------------------------------------------------------------

pub struct JsonStringifyNode;
impl NodeDescriptor for JsonStringifyNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.json.stringify".into(),
			title: "JSON Stringify".into(),
			category: "json".into(),
			description: Some("Json → String（pretty=true で整形出力）".into()),
			inputs: vec![
				PortSpec::input("value", "Value", SocketType::Json),
				PortSpec::input("pretty", "Pretty", SocketType::Bool).with_default(SocketValue::Bool(false)),
			],
			outputs: vec![PortSpec::output("text", "Text", SocketType::String)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for JsonStringifyNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let v = get_required_json(inputs, "value")?;
		let pretty = get_optional_bool(inputs, "pretty", false)?;
		let s = if pretty {
			serde_json::to_string_pretty(v).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("stringify: {e}")))?
		} else {
			serde_json::to_string(v).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("stringify: {e}")))?
		};
		Ok(NodeOutput::new().set_data("text", SocketValue::String(s)))
	}
}

// ----- get (dot path) -------------------------------------------------------

pub struct JsonGetNode;
impl NodeDescriptor for JsonGetNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.json.get".into(),
			title: "JSON Get".into(),
			category: "json".into(),
			description: Some("ドットパス（foo.bar[0].baz 等）で値を抜き出す。存在しない場合は Null".into()),
			inputs: vec![
				PortSpec::input("value", "Value", SocketType::Json),
				PortSpec::input("path", "Path", SocketType::String),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Json)],
			properties: vec![],
		}
	}
}
#[async_trait]
impl PureNode for JsonGetNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let v = get_required_json(inputs, "value")?;
		let path = get_required_string(inputs, "path")?;
		let resolved = walk_dot_path(v, &path);
		Ok(NodeOutput::new().set_data("result", SocketValue::Json(resolved)))
	}
}

fn walk_dot_path(v: &serde_json::Value, path: &str) -> serde_json::Value {
	if path.is_empty() {
		return v.clone();
	}
	let mut cur = v;
	// 分解: "." で区切った後、各断片の末尾にある `[N]` を分離する
	for seg in path.split('.') {
		let (key, indices) = split_indices(seg);
		// key 部分がある場合は先に object として掘る
		if !key.is_empty() {
			match cur.get(key) {
				Some(next) => cur = next,
				None => return serde_json::Value::Null,
			}
		}
		for idx in indices {
			match cur.get(idx) {
				Some(next) => cur = next,
				None => return serde_json::Value::Null,
			}
		}
	}
	cur.clone()
}

/// "foo[0][1]" → ("foo", [0, 1])
fn split_indices(seg: &str) -> (&str, Vec<usize>) {
	let mut indices = Vec::new();
	let (key, mut rest) = match seg.find('[') {
		Some(pos) => seg.split_at(pos),
		None => return (seg, indices),
	};
	while !rest.is_empty() {
		if !rest.starts_with('[') {
			break;
		}
		let Some(close) = rest.find(']') else { break };
		let num = &rest[1..close];
		match num.parse::<usize>() {
			Ok(n) => indices.push(n),
			Err(_) => break,
		}
		rest = &rest[close + 1..];
	}
	(key, indices)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn parse_and_stringify_roundtrip() {
		let inputs: InputMap = [("text".into(), SocketValue::String(r#"{"x": 1}"#.into()))].into_iter().collect();
		let out = JsonParseNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let v = out.data.get("value").cloned().unwrap();
		assert_eq!(v, SocketValue::Json(serde_json::json!({"x": 1})));

		let inputs: InputMap = [("value".into(), v)].into_iter().collect();
		let out = JsonStringifyNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("text"), Some(&SocketValue::String(r#"{"x":1}"#.into())));
	}

	#[tokio::test]
	async fn get_by_dot_path() {
		let value = SocketValue::Json(serde_json::json!({"a": {"b": [10, 20, 30], "c": "hi"}}));
		let inputs: InputMap = [
			("value".into(), value.clone()),
			("path".into(), SocketValue::String("a.b[1]".into())),
		]
		.into_iter()
		.collect();
		let out = JsonGetNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Json(serde_json::json!(20))));

		let inputs: InputMap = [("value".into(), value.clone()), ("path".into(), SocketValue::String("a.c".into()))]
			.into_iter()
			.collect();
		let out = JsonGetNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Json(serde_json::json!("hi"))));

		let inputs: InputMap = [("value".into(), value), ("path".into(), SocketValue::String("a.missing".into()))]
			.into_iter()
			.collect();
		let out = JsonGetNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Json(serde_json::Value::Null)));
	}

	#[tokio::test]
	async fn parse_invalid_errors() {
		let inputs: InputMap = [("text".into(), SocketValue::String("not json".into()))].into_iter().collect();
		let e = JsonParseNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn try_parse_returns_result_for_success_and_error() {
		let inputs: InputMap = [("text".into(), SocketValue::String(r#"{"x": 1}"#.into()))].into_iter().collect();
		let out = JsonTryParseNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("ok"), Some(&SocketValue::Bool(true)));
		let result = out.data.get("result").unwrap().as_result().unwrap();
		assert!(result.ok);
		assert_eq!(result.value.as_ref().unwrap().as_json().unwrap()["x"], 1);

		let inputs: InputMap = [("text".into(), SocketValue::String("not json".into()))].into_iter().collect();
		let out = JsonTryParseNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("ok"), Some(&SocketValue::Bool(false)));
		assert!(matches!(out.data.get("value"), Some(SocketValue::Json(serde_json::Value::Null))));
		let result = out.data.get("result").unwrap().as_result().unwrap();
		assert!(!result.ok);
		assert_eq!(result.code.as_deref(), Some("json.parse"));
		assert!(result.error.as_deref().unwrap().contains("JSON parse error"));
	}
}
