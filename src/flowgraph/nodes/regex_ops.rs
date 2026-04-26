//! `flowgraph.regex.replace`: V1 の `Modify` が持っていた「正規表現置換」を担う PureNode。
//!
//! ## ポート
//!
//! - 入力:
//!   - `content` (String)
//!   - `rules` (List<Json>): 各要素 `{"pattern": "...", "replacement": "..."}`
//! - 出力:
//!   - `result` (String): 置換後文字列
//!   - `errors` (List<String>): コンパイル失敗したパターンの説明。致命ではなくスキップされる。
//!
//! ## 設計メモ
//!
//! - 正規表現のコンパイルは毎回行う。将来的に engine のメモ化と組み合わせる。
//!   ホットパスで気になるなら、ユーザ側で `state.latch` に compiled table をキャッシュする構成が可能。
//! - V1 は `from_regex.replace_all(&content, replacer)` と `&str` を replacer として使っていたため、
//!   ここでもその挙動を踏襲する（`$1` 等のキャプチャ置換も有効）。

use crate::flowgraph::node::{
	get_required_list, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct RegexReplaceNode;

impl NodeDescriptor for RegexReplaceNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.regex.replace".into(),
			title: "Regex Replace".into(),
			category: "regex".into(),
			description: Some("正規表現ルール（{pattern, replacement}）で content を逐次置換する".into()),
			inputs: vec![
				PortSpec::input("content", "Content", SocketType::String),
				PortSpec::input("rules", "Rules", SocketType::List(Box::new(SocketType::Json))).with_default(SocketValue::List(vec![])),
			],
			outputs: vec![
				PortSpec::output("result", "Result", SocketType::String),
				PortSpec::output("errors", "Errors", SocketType::List(Box::new(SocketType::String))),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for RegexReplaceNode {
	async fn compute(&self, _props: &InputMap, inputs: &InputMap, _fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let mut content = get_required_string(inputs, "content")?;
		let rules = get_required_list(inputs, "rules")?;
		let mut errors: Vec<SocketValue> = Vec::new();

		for (idx, entry) in rules.iter().enumerate() {
			let j = match entry {
				SocketValue::Json(j) => j,
				other => {
					errors.push(SocketValue::String(format!(
						"rules[{idx}]: expected Json object, got {:?}",
						other.type_of()
					)));
					continue;
				}
			};
			let Some(obj) = j.as_object() else {
				errors.push(SocketValue::String(format!("rules[{idx}]: expected object, got {j}")));
				continue;
			};
			let Some(pattern) = obj.get("pattern").and_then(|v| v.as_str()) else {
				errors.push(SocketValue::String(format!("rules[{idx}]: 'pattern' missing or not string")));
				continue;
			};
			let replacement = obj.get("replacement").and_then(|v| v.as_str()).unwrap_or("");
			match regex::Regex::new(pattern) {
				Ok(re) => {
					content = re.replace_all(&content, replacement).to_string();
				}
				Err(e) => {
					errors.push(SocketValue::String(format!("rules[{idx}]: invalid pattern '{pattern}': {e}")));
				}
			}
		}

		Ok(NodeOutput::new()
			.set_data("result", SocketValue::String(content))
			.set_data("errors", SocketValue::List(errors)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn rule(pattern: &str, replacement: &str) -> SocketValue {
		SocketValue::Json(json!({"pattern": pattern, "replacement": replacement}))
	}

	#[tokio::test]
	async fn replace_basic_and_captures() {
		let node = RegexReplaceNode;
		let rules = SocketValue::List(vec![rule(r"(\w+)@(\w+)", "$1 at $2"), rule(r"\s+", " ")]);
		let inputs: InputMap = [
			("content".into(), SocketValue::String("foo@bar   baz@qux".into())),
			("rules".into(), rules),
		]
		.into_iter()
		.collect();
		let out = node.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::String("foo at bar baz at qux".into())));
		assert_eq!(out.data.get("errors"), Some(&SocketValue::List(vec![])));
	}

	#[tokio::test]
	async fn invalid_pattern_reported_not_fatal() {
		let node = RegexReplaceNode;
		let rules = SocketValue::List(vec![rule("[unclosed", "x"), rule(r"\d+", "N")]);
		let inputs: InputMap = [
			("content".into(), SocketValue::String("a1 b22 c333".into())),
			("rules".into(), rules),
		]
		.into_iter()
		.collect();
		let out = node.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::String("aN bN cN".into())));
		match out.data.get("errors").unwrap() {
			SocketValue::List(xs) => assert_eq!(xs.len(), 1),
			_ => panic!(),
		}
	}

	#[tokio::test]
	async fn empty_rules_is_identity() {
		let node = RegexReplaceNode;
		let inputs: InputMap = [
			("content".into(), SocketValue::String("unchanged".into())),
			("rules".into(), SocketValue::List(vec![])),
		]
		.into_iter()
		.collect();
		let out = node.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::String("unchanged".into())));
	}
}
