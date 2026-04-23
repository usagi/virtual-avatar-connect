//! Responses API DTO の serde roundtrip / golden JSON テスト。
//!
//! OpenAI 公式 docs の典型的な payload 形状を golden として書き留めておく。
//! API の field 追加には `#[serde(default)]` + `#[serde(other)]` で前向き互換を維持する
//! 設計なので、テストが壊れた場合はまず API 側の破壊的変更を疑う。

use serde_json::json;

use crate::ai::openai_responses::types::{
 CreateResponseRequest, ErrorObject, InputContent, InputItem, MessageContent, OutputItem,
 Reasoning, ReasoningEffort, Response, ResponseStatus, TextConfig, TextFormat, Tool, ToolChoice,
 ToolChoiceMode, Usage,
};

#[test]
fn serialize_minimal_request()
{
 let req = CreateResponseRequest {
  model: "gpt-4.1-mini".to_string(),
  input: vec![InputItem::message("user", "Hello")],
  ..Default::default()
 };
 let v = serde_json::to_value(&req).unwrap();
 assert_eq!(v["model"], "gpt-4.1-mini");
 assert_eq!(v["input"].as_array().unwrap().len(), 1);
 assert_eq!(v["input"][0]["type"], "message");
 assert_eq!(v["input"][0]["role"], "user");
 assert_eq!(v["input"][0]["content"], "Hello");
 assert!(v.get("max_output_tokens").is_none(), "None fields must be skipped");
 assert!(v.get("reasoning").is_none());
 assert!(v.get("tools").is_none());
}

#[test]
fn serialize_full_request_with_reasoning_and_tools()
{
 let req = CreateResponseRequest {
  model: "gpt-5-mini".to_string(),
  input: vec![
   InputItem::Message {
    role: "developer".to_string(),
    content: InputContent::Text("You are VAC.".to_string()),
   },
   InputItem::Message {
    role: "user".to_string(),
    content: InputContent::Text("Ping".to_string()),
   },
  ],
  max_output_tokens: Some(256),
  // 0.7 は f32→f64 で丸め誤差が出る（0.699999988079071）ので、
  // golden 比較は 2 進数で丸めなく表せる 0.5 を使う。
  temperature: Some(0.5),
  reasoning: Some(Reasoning {
   effort: Some(ReasoningEffort::Medium),
   summary: None,
  }),
  text: Some(TextConfig {
   format: Some(TextFormat::JsonObject),
  }),
  tools: Some(vec![Tool::Function {
   name: "vac_ping".to_string(),
   description: Some("Returns pong".to_string()),
   parameters: json!({ "type": "object", "properties": {} }),
   strict: Some(true),
  }]),
  tool_choice: Some(ToolChoice::Mode(ToolChoiceMode::Auto)),
  stream: Some(false),
  store: Some(false),
  ..Default::default()
 };
 let v = serde_json::to_value(&req).unwrap();
 assert_eq!(v["model"], "gpt-5-mini");
 assert_eq!(v["max_output_tokens"], 256);
 assert_eq!(v["temperature"], 0.5);
 assert_eq!(v["reasoning"]["effort"], "medium");
 assert_eq!(v["text"]["format"]["type"], "json_object");
 assert_eq!(v["tools"][0]["type"], "function");
 assert_eq!(v["tools"][0]["name"], "vac_ping");
 assert_eq!(v["tools"][0]["strict"], true);
 assert_eq!(v["tool_choice"], "auto");
 assert_eq!(v["stream"], false);
 assert_eq!(v["store"], false);
}

#[test]
fn roundtrip_function_call_output_input_item()
{
 let item = InputItem::function_call_output("call_abc", "{\"result\":42}");
 let v = serde_json::to_value(&item).unwrap();
 assert_eq!(v["type"], "function_call_output");
 assert_eq!(v["call_id"], "call_abc");
 assert_eq!(v["output"], "{\"result\":42}");
 let back: InputItem = serde_json::from_value(v).unwrap();
 assert_eq!(back, item);
}

#[test]
fn roundtrip_function_call_input_item()
{
 let item = InputItem::FunctionCall {
  call_id: "call_abc".to_string(),
  name: "vac_ping".to_string(),
  arguments: "{}".to_string(),
 };
 let v = serde_json::to_value(&item).unwrap();
 assert_eq!(v["type"], "function_call");
 let back: InputItem = serde_json::from_value(v).unwrap();
 assert_eq!(back, item);
}

#[test]
fn deserialize_response_with_message_output()
{
 let raw = json!({
  "id": "resp_abc123",
  "object": "response",
  "created_at": 1_713_000_000_i64,
  "status": "completed",
  "error": null,
  "model": "gpt-4.1-mini-2025-04-14",
  "output": [
   {
    "id": "msg_xyz",
    "type": "message",
    "status": "completed",
    "role": "assistant",
    "content": [
     {
      "type": "output_text",
      "text": "Hello! How can I help?",
      "annotations": []
     }
    ]
   }
  ],
  "output_text": "Hello! How can I help?",
  "usage": {
   "input_tokens": 10,
   "output_tokens": 8,
   "total_tokens": 18,
   "input_tokens_details": { "cached_tokens": 0 },
   "output_tokens_details": { "reasoning_tokens": 0 }
  }
 });
 let res: Response = serde_json::from_value(raw).unwrap();
 assert_eq!(res.id, "resp_abc123");
 assert_eq!(res.status, Some(ResponseStatus::Completed));
 assert_eq!(res.output_text.as_deref(), Some("Hello! How can I help?"));
 assert_eq!(res.output.len(), 1);
 match &res.output[0]
 {
  OutputItem::Message { role, content, .. } =>
  {
   assert_eq!(role, "assistant");
   assert_eq!(content.len(), 1);
   match &content[0]
   {
    MessageContent::OutputText { text, .. } => assert_eq!(text, "Hello! How can I help?"),
    _ => panic!("expected OutputText"),
   }
  }
  _ => panic!("expected Message"),
 }
 let usage = res.usage.unwrap();
 assert_eq!(usage.input_tokens, 10);
 assert_eq!(usage.output_tokens, 8);
 assert_eq!(usage.total_tokens, 18);
}

#[test]
fn deserialize_response_with_function_call_output()
{
 let raw = json!({
  "id": "resp_fc",
  "object": "response",
  "status": "completed",
  "model": "gpt-4.1-mini",
  "output": [
   {
    "id": "fc_1",
    "type": "function_call",
    "status": "completed",
    "call_id": "call_123",
    "name": "vac_ping",
    "arguments": "{\"nonce\":42}"
   }
  ],
  "output_text": null
 });
 let res: Response = serde_json::from_value(raw).unwrap();
 assert_eq!(res.output.len(), 1);
 let fc = res.output[0].as_function_call().expect("function_call view");
 assert_eq!(fc.call_id, "call_123");
 assert_eq!(fc.name, "vac_ping");
 assert_eq!(fc.arguments, "{\"nonce\":42}");
}

#[test]
fn deserialize_response_with_reasoning_and_mixed_output()
{
 let raw = json!({
  "id": "resp_mixed",
  "status": "completed",
  "model": "gpt-5-mini",
  "output": [
   {
    "id": "rs_1",
    "type": "reasoning",
    "status": "completed",
    "summary": [{"type": "text", "text": "thinking..."}]
   },
   {
    "id": "msg_1",
    "type": "message",
    "status": "completed",
    "role": "assistant",
    "content": [
     {"type": "output_text", "text": "Answer."}
    ]
   }
  ]
 });
 let res: Response = serde_json::from_value(raw).unwrap();
 assert_eq!(res.output.len(), 2);
 assert!(matches!(res.output[0], OutputItem::Reasoning { .. }));
 assert!(matches!(res.output[1], OutputItem::Message { .. }));
}

#[test]
fn deserialize_response_with_unknown_output_item_type()
{
 let raw = json!({
  "id": "resp_unknown",
  "status": "completed",
  "model": "gpt-x",
  "output": [
   {
    "id": "wsc_1",
    "type": "web_search_call",
    "status": "completed",
    "query": "vac"
   }
  ]
 });
 let res: Response = serde_json::from_value(raw).unwrap();
 assert_eq!(res.output.len(), 1);
 assert!(matches!(res.output[0], OutputItem::Other));
}

#[test]
fn deserialize_response_with_error_object()
{
 let raw = json!({
  "id": "resp_err",
  "status": "failed",
  "model": "gpt-4.1-mini",
  "error": {
   "code": "rate_limit_exceeded",
   "message": "Rate limit reached",
   "type": "rate_limit_error",
   "param": null
  },
  "output": []
 });
 let res: Response = serde_json::from_value(raw).unwrap();
 assert_eq!(res.status, Some(ResponseStatus::Failed));
 let err: ErrorObject = res.error.expect("error present");
 assert_eq!(err.code.as_deref(), Some("rate_limit_exceeded"));
 assert_eq!(err.kind.as_deref(), Some("rate_limit_error"));
 assert_eq!(err.message.as_deref(), Some("Rate limit reached"));
}

#[test]
fn deserialize_usage_with_missing_details()
{
 let raw = json!({
  "input_tokens": 5,
  "output_tokens": 3,
  "total_tokens": 8
 });
 let u: Usage = serde_json::from_value(raw).unwrap();
 assert_eq!(u.input_tokens, 5);
 assert_eq!(u.output_tokens, 3);
 assert_eq!(u.total_tokens, 8);
 assert!(u.input_tokens_details.is_none());
 assert!(u.output_tokens_details.is_none());
}

#[test]
fn deserialize_response_tolerates_missing_fields()
{
 let raw = json!({ "id": "resp_min" });
 let res: Response = serde_json::from_value(raw).unwrap();
 assert_eq!(res.id, "resp_min");
 assert!(res.status.is_none());
 assert!(res.output.is_empty());
 assert!(res.output_text.is_none());
}

#[test]
fn tool_choice_named_function_serializes_with_type_tag()
{
 use crate::ai::openai_responses::types::NamedToolChoice;
 let choice = ToolChoice::Named(NamedToolChoice::Function {
  name: "vac_ping".to_string(),
 });
 let v = serde_json::to_value(&choice).unwrap();
 assert_eq!(v["type"], "function");
 assert_eq!(v["name"], "vac_ping");
}

#[test]
fn text_format_json_schema_roundtrip()
{
 let tf = TextFormat::JsonSchema {
  name: "vac_reply".to_string(),
  schema: json!({"type": "object"}),
  strict: Some(true),
  description: None,
 };
 let v = serde_json::to_value(&tf).unwrap();
 assert_eq!(v["type"], "json_schema");
 assert_eq!(v["name"], "vac_reply");
 assert_eq!(v["strict"], true);
 assert!(v.get("description").is_none());
 let back: TextFormat = serde_json::from_value(v).unwrap();
 assert_eq!(back, tf);
}
