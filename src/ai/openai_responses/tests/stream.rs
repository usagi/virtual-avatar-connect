//! SSE parser / StreamEvent dispatch の単体テスト。
//!
//! `parse_event_data(event_type, data_json)` は純関数なので、OpenAI の実
//! サーバに繋がずに典型 payload を流して検証する。
//!
//! `stream_events` は `bytes::Bytes` の stream から SSE をパースする統合テスト相当を
//! 最後に 1 件だけ置き、`[DONE]` 終端・空 data スキップ・複数イベント直列投入の挙動を
//! まとめて確かめる。

use bytes::Bytes;
use futures::{stream, StreamExt};

use crate::ai::openai_responses::sse::{parse_event_data, stream_events, SseError};
use crate::ai::openai_responses::types::stream::StreamEvent;
use crate::ai::openai_responses::types::{MessageContent, OutputItem, ResponseStatus};

// ------------------------------------------------------------
// parse_event_data: 個別イベント種別
// ------------------------------------------------------------

#[test]
fn parse_created_event() {
	let data = r#"{
  "type": "response.created",
  "response": {
   "id": "resp_abc",
   "object": "response",
   "created_at": 1700000000,
   "status": "in_progress",
   "model": "gpt-4.1-mini",
   "output": []
  }
 }"#;
	let ev = parse_event_data("response.created", data).unwrap();
	match ev {
		StreamEvent::Created { response } => {
			assert_eq!(response.id, "resp_abc");
			assert_eq!(response.status, Some(ResponseStatus::InProgress));
		}
		other => panic!("expected Created, got {other:?}"),
	}
}

#[test]
fn parse_in_progress_event() {
	let data = r#"{
  "type": "response.in_progress",
  "response": {"id": "resp_ip", "status": "in_progress"}
 }"#;
	let ev = parse_event_data("response.in_progress", data).unwrap();
	assert!(matches!(ev, StreamEvent::InProgress { .. }));
}

#[test]
fn parse_output_item_added_message() {
	let data = r#"{
  "type": "response.output_item.added",
  "output_index": 0,
  "item": {
   "id": "msg_xyz",
   "type": "message",
   "status": "in_progress",
   "role": "assistant",
   "content": []
  }
 }"#;
	let ev = parse_event_data("response.output_item.added", data).unwrap();
	match ev {
		StreamEvent::OutputItemAdded { output_index, item } => {
			assert_eq!(output_index, 0);
			assert!(matches!(item, OutputItem::Message { .. }));
		}
		other => panic!("expected OutputItemAdded, got {other:?}"),
	}
}

#[test]
fn parse_output_item_added_function_call() {
	let data = r#"{
  "type": "response.output_item.added",
  "output_index": 1,
  "item": {
   "id": "fc_1",
   "type": "function_call",
   "status": "in_progress",
   "call_id": "call_abc",
   "name": "vac_ping",
   "arguments": ""
  }
 }"#;
	let ev = parse_event_data("response.output_item.added", data).unwrap();
	match ev {
		StreamEvent::OutputItemAdded { output_index, item } => {
			assert_eq!(output_index, 1);
			let fc = item.as_function_call().expect("function_call view");
			assert_eq!(fc.call_id, "call_abc");
			assert_eq!(fc.name, "vac_ping");
			assert_eq!(fc.arguments, "");
		}
		other => panic!("expected OutputItemAdded, got {other:?}"),
	}
}

#[test]
fn parse_output_text_delta() {
	let data = r#"{
  "type": "response.output_text.delta",
  "item_id": "msg_xyz",
  "output_index": 0,
  "content_index": 0,
  "delta": "Hello"
 }"#;
	let ev = parse_event_data("response.output_text.delta", data).unwrap();
	match ev {
		StreamEvent::OutputTextDelta {
			item_id,
			output_index,
			content_index,
			delta,
		} => {
			assert_eq!(item_id, "msg_xyz");
			assert_eq!(output_index, 0);
			assert_eq!(content_index, 0);
			assert_eq!(delta, "Hello");
		}
		other => panic!("expected OutputTextDelta, got {other:?}"),
	}
}

#[test]
fn parse_output_text_done() {
	let data = r#"{
  "type": "response.output_text.done",
  "item_id": "msg_xyz",
  "output_index": 0,
  "content_index": 0,
  "text": "Hello, world."
 }"#;
	let ev = parse_event_data("response.output_text.done", data).unwrap();
	match ev {
		StreamEvent::OutputTextDone { text, .. } => assert_eq!(text, "Hello, world."),
		other => panic!("expected OutputTextDone, got {other:?}"),
	}
}

#[test]
fn parse_function_call_arguments_delta() {
	let data = r#"{
  "type": "response.function_call_arguments.delta",
  "item_id": "fc_1",
  "output_index": 1,
  "delta": "{\"loc"
 }"#;
	let ev = parse_event_data("response.function_call_arguments.delta", data).unwrap();
	match ev {
		StreamEvent::FunctionCallArgumentsDelta { item_id, delta, .. } => {
			assert_eq!(item_id, "fc_1");
			assert_eq!(delta, "{\"loc");
		}
		other => panic!("expected FunctionCallArgumentsDelta, got {other:?}"),
	}
}

#[test]
fn parse_function_call_arguments_done() {
	let data = r#"{
  "type": "response.function_call_arguments.done",
  "item_id": "fc_1",
  "output_index": 1,
  "arguments": "{\"location\":\"Tokyo\"}"
 }"#;
	let ev = parse_event_data("response.function_call_arguments.done", data).unwrap();
	match ev {
		StreamEvent::FunctionCallArgumentsDone { arguments, .. } => {
			let parsed: serde_json::Value = serde_json::from_str(&arguments).unwrap();
			assert_eq!(parsed["location"], "Tokyo");
		}
		other => panic!("expected FunctionCallArgumentsDone, got {other:?}"),
	}
}

#[test]
fn parse_output_item_done_function_call() {
	let data = r#"{
  "type": "response.output_item.done",
  "output_index": 1,
  "item": {
   "id": "fc_1",
   "type": "function_call",
   "status": "completed",
   "call_id": "call_abc",
   "name": "vac_ping",
   "arguments": "{\"nonce\":42}"
  }
 }"#;
	let ev = parse_event_data("response.output_item.done", data).unwrap();
	match ev {
		StreamEvent::OutputItemDone { item, .. } => {
			let fc = item.as_function_call().expect("function_call view");
			assert_eq!(fc.arguments, "{\"nonce\":42}");
		}
		other => panic!("expected OutputItemDone, got {other:?}"),
	}
}

#[test]
fn parse_completed_event_with_usage() {
	let data = r#"{
  "type": "response.completed",
  "response": {
   "id": "resp_done",
   "status": "completed",
   "model": "gpt-4.1-mini",
   "output": [
    {
     "id": "msg_1",
     "type": "message",
     "role": "assistant",
     "status": "completed",
     "content": [{"type": "output_text", "text": "done"}]
    }
   ],
   "usage": {"input_tokens": 3, "output_tokens": 1, "total_tokens": 4}
  }
 }"#;
	let ev = parse_event_data("response.completed", data).unwrap();
	match ev {
		StreamEvent::Completed { response } => {
			assert_eq!(response.id, "resp_done");
			assert_eq!(response.status, Some(ResponseStatus::Completed));
			let usage = response.usage.expect("usage");
			assert_eq!(usage.total_tokens, 4);
			assert_eq!(response.output.len(), 1);
			match &response.output[0] {
				OutputItem::Message { content, .. } => match &content[0] {
					MessageContent::OutputText { text, .. } => assert_eq!(text, "done"),
					other => panic!("expected OutputText, got {other:?}"),
				},
				other => panic!("expected Message, got {other:?}"),
			}
		}
		other => panic!("expected Completed, got {other:?}"),
	}
}

#[test]
fn parse_failed_event() {
	let data = r#"{
  "type": "response.failed",
  "response": {
   "id": "resp_failed",
   "status": "failed",
   "error": {
    "code": "server_error",
    "message": "boom",
    "type": "server_error"
   }
  }
 }"#;
	let ev = parse_event_data("response.failed", data).unwrap();
	match ev {
		StreamEvent::Failed { response } => {
			assert_eq!(response.status, Some(ResponseStatus::Failed));
			let err = response.error.expect("error");
			assert_eq!(err.code.as_deref(), Some("server_error"));
		}
		other => panic!("expected Failed, got {other:?}"),
	}
}

#[test]
fn parse_incomplete_event() {
	let data = r#"{
  "type": "response.incomplete",
  "response": {
   "id": "resp_inc",
   "status": "incomplete",
   "incomplete_details": {"reason": "max_output_tokens"}
  }
 }"#;
	let ev = parse_event_data("response.incomplete", data).unwrap();
	match ev {
		StreamEvent::Incomplete { response } => {
			assert_eq!(response.status, Some(ResponseStatus::Incomplete));
			assert_eq!(response.incomplete_details.unwrap().reason.as_deref(), Some("max_output_tokens"));
		}
		other => panic!("expected Incomplete, got {other:?}"),
	}
}

#[test]
fn parse_top_level_error_event() {
	let data = r#"{
  "type": "error",
  "error": {
   "code": "invalid_request_error",
   "message": "missing model",
   "type": "invalid_request_error",
   "param": "model"
  }
 }"#;
	let ev = parse_event_data("error", data).unwrap();
	match ev {
		StreamEvent::Error { error } => {
			assert_eq!(error.code.as_deref(), Some("invalid_request_error"));
			assert_eq!(error.param.as_deref(), Some("model"));
		}
		other => panic!("expected Error, got {other:?}"),
	}
}

#[test]
fn parse_unknown_event_falls_through_to_other() {
	let data = r#"{
  "type": "response.web_search_call.in_progress",
  "item_id": "wsc_1"
 }"#;
	let ev = parse_event_data("response.web_search_call.in_progress", data).unwrap();
	match ev {
		StreamEvent::Other { raw_type } => {
			assert_eq!(raw_type, "response.web_search_call.in_progress");
		}
		other => panic!("expected Other, got {other:?}"),
	}
}

#[test]
fn parse_uses_data_type_if_event_header_missing() {
	let data = r#"{
  "type": "response.output_text.delta",
  "item_id": "msg_1",
  "output_index": 0,
  "content_index": 0,
  "delta": "hi"
 }"#;
	let ev = parse_event_data("", data).unwrap();
	assert!(matches!(ev, StreamEvent::OutputTextDelta { .. }));
}

#[test]
fn parse_returns_json_error_on_malformed_data() {
	let err = parse_event_data("response.created", "{ not json").unwrap_err();
	match err {
		SseError::Json { kind, .. } => assert_eq!(kind, "response.created"),
		other => panic!("expected Json error, got {other:?}"),
	}
}

#[test]
fn parse_returns_json_error_on_missing_required_field() {
	// response.created は `response` field が必須。無い場合はエラー。
	let err = parse_event_data("response.created", "{\"type\":\"response.created\"}").unwrap_err();
	assert!(matches!(err, SseError::Json { .. }));
}

// ------------------------------------------------------------
// parse_event_data: function_call の delta accumulate → done で JSON parse
// ------------------------------------------------------------

#[test]
fn function_call_delta_accumulates_across_chunks() {
	let deltas = ["{\"lo", "cation\":", "\"Tokyo\"}"];
	let mut acc = String::new();
	for d in deltas {
		let data = format!(
			r#"{{"type":"response.function_call_arguments.delta","item_id":"fc_1","output_index":1,"delta":"{}"}}"#,
			d.replace('"', "\\\"")
		);
		let ev = parse_event_data("response.function_call_arguments.delta", &data).unwrap();
		if let StreamEvent::FunctionCallArgumentsDelta { delta, .. } = ev {
			acc.push_str(&delta);
		} else {
			panic!("expected FunctionCallArgumentsDelta");
		}
	}
	let parsed: serde_json::Value = serde_json::from_str(&acc).unwrap();
	assert_eq!(parsed["location"], "Tokyo");
}

// ------------------------------------------------------------
// stream_events: bytes stream 統合
// ------------------------------------------------------------

fn bytes(s: &str) -> Bytes {
	Bytes::from(s.to_string())
}

#[tokio::test]
async fn stream_events_parses_multi_event_payload() {
	let sse = "\
event: response.created\n\
data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"status\":\"in_progress\"}}\n\
\n\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"Hel\"}\n\
\n\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"lo\"}\n\
\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\"}}\n\
\n\
data: [DONE]\n\
\n";

	let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(bytes(sse))];
	let byte_stream = stream::iter(chunks);
	let events: Vec<Result<StreamEvent, SseError>> = stream_events(byte_stream).collect().await;
	let events: Vec<StreamEvent> = events.into_iter().map(|r| r.unwrap()).collect();
	assert_eq!(events.len(), 4, "[DONE] を除いて 4 イベント");
	assert!(matches!(events[0], StreamEvent::Created { .. }));
	match &events[1] {
		StreamEvent::OutputTextDelta { delta, .. } => assert_eq!(delta, "Hel"),
		other => panic!("expected Delta, got {other:?}"),
	}
	match &events[2] {
		StreamEvent::OutputTextDelta { delta, .. } => assert_eq!(delta, "lo"),
		other => panic!("expected Delta, got {other:?}"),
	}
	assert!(matches!(events[3], StreamEvent::Completed { .. }));
}

#[tokio::test]
async fn stream_events_skips_empty_data_and_done_sentinel() {
	let sse = "\
data: \n\
\n\
data: [DONE]\n\
\n";
	let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(bytes(sse))];
	let byte_stream = stream::iter(chunks);
	let events: Vec<_> = stream_events(byte_stream).collect().await;
	assert!(events.is_empty(), "empty data と [DONE] は全てスキップ");
}

#[tokio::test]
async fn stream_events_tolerates_split_chunks() {
	// 1 イベントを 3 チャンクに分割しても eventsource-stream が繋ぎ合わせる。
	let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
		Ok(bytes("event: response.output_text.delta\n")),
		Ok(bytes("data: {\"type\":\"response.output_text.delta\",\"item_id\":\"m\",")),
		Ok(bytes("\"output_index\":0,\"content_index\":0,\"delta\":\"x\"}\n\n")),
	];
	let byte_stream = stream::iter(chunks);
	let events: Vec<Result<StreamEvent, SseError>> = stream_events(byte_stream).collect().await;
	assert_eq!(events.len(), 1);
	match events.into_iter().next().unwrap().unwrap() {
		StreamEvent::OutputTextDelta { delta, .. } => assert_eq!(delta, "x"),
		other => panic!("expected Delta, got {other:?}"),
	}
}
