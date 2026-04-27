//! SSE ストリーミングイベント型。
//!
//! OpenAI Responses API の SSE は 40+ のイベント種別を持つが、VAC が実際に
//! 意味ある処理をするのは以下のサブセットのみ。未知イベントは [`StreamEvent::Other`]
//! に落として前向き互換を確保する。
//!
//! # 主要イベント
//!
//! | OpenAI event type | `StreamEvent` | VAC 側の使い方 |
//! |---|---|---|
//! | `response.created` | [`StreamEvent::Created`] | response_id 取得、ログ |
//! | `response.in_progress` | [`StreamEvent::InProgress`] | デバッグログのみ |
//! | `response.output_item.added` | [`StreamEvent::OutputItemAdded`] | message / function_call 開始検知 |
//! | `response.output_text.delta` | [`StreamEvent::OutputTextDelta`] | assistant text のストリーム表示 |
//! | `response.output_text.done` | [`StreamEvent::OutputTextDone`] | text item 完了 |
//! | `response.function_call_arguments.delta` | [`StreamEvent::FunctionCallArgumentsDelta`] | function_call 引数 accumulator |
//! | `response.function_call_arguments.done` | [`StreamEvent::FunctionCallArgumentsDone`] | 引数完了、JSON parse |
//! | `response.output_item.done` | [`StreamEvent::OutputItemDone`] | 完成 item（ツール呼出成果の再構成） |
//! | `response.completed` | [`StreamEvent::Completed`] | 最終 usage / 完了 |
//! | `response.incomplete` | [`StreamEvent::Incomplete`] | max_output_tokens 到達等 |
//! | `response.failed` | [`StreamEvent::Failed`] | エラー扱いで abort |
//! | `error` | [`StreamEvent::Error`] | top-level エラー（response 開始前の失敗等） |
//!
//! 詳細: `docs/roadmap/phase-chi-openai-responses.md` §5。
//!
//! 各 variant の payload は SSE パースで保持するが、VAC 側の分岐は一部フィールドのみ参照する。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use super::response::{ErrorObject, OutputItem, Response};

/// Responses SSE の 1 イベント。
///
/// `Response` 型が `PartialEq` を実装しないため `StreamEvent` 自身も `PartialEq` を持たない。
/// 比較したい場合はテストで `matches!` + 個別フィールドアクセスを使うこと。
#[derive(Debug, Clone)]
pub enum StreamEvent {
	/// `response.created` — stream 開始直後、response の初期 state。
	Created { response: Response },
	/// `response.in_progress` — stream 進行中の周期通知（content は増えていない）。
	InProgress { response: Response },
	/// `response.output_item.added` — 新しい output item（message / function_call /
	/// reasoning 等）が開始された。
	OutputItemAdded { output_index: u32, item: OutputItem },
	/// `response.output_item.done` — output item が完成した。message の場合は最終テキスト、
	/// function_call の場合は引数を含む完成形。
	OutputItemDone { output_index: u32, item: OutputItem },
	/// `response.output_text.delta` — assistant text の 1 デルタ。
	OutputTextDelta {
		item_id: String,
		output_index: u32,
		content_index: u32,
		delta: String,
	},
	/// `response.output_text.done` — text content part の完成テキスト。
	OutputTextDone {
		item_id: String,
		output_index: u32,
		content_index: u32,
		text: String,
	},
	/// `response.function_call_arguments.delta` — function_call の引数部分文字列。
	/// 途中で JSON parse を試みず、[`StreamEvent::FunctionCallArgumentsDone`] で一括 parse する。
	FunctionCallArgumentsDelta { item_id: String, output_index: u32, delta: String },
	/// `response.function_call_arguments.done` — function_call の引数完成。
	FunctionCallArgumentsDone {
		item_id: String,
		output_index: u32,
		arguments: String,
	},
	/// `response.completed` — stream 終了時の最終 response。usage 含む。
	Completed { response: Response },
	/// `response.incomplete` — max_output_tokens 到達等。
	Incomplete { response: Response },
	/// `response.failed` — サーバ側失敗。`response.error` を含む。
	Failed { response: Response },
	/// Top-level `error` イベント（`response.created` 前の失敗や transport レベルのエラー通知）。
	Error { error: ErrorObject },
	/// 未知イベント catch-all。
	/// 将来 OpenAI が追加するイベントはここに落ち、ログに raw_type を出して以降の処理を続行する。
	Other { raw_type: String },
}

/// 共通 payload: `response.*` 系イベントでしばしば現れる `response` / `type` だけの形。
/// デシリアライズ用ヘルパ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResponseEnvelope {
	#[serde(rename = "type", default)]
	pub r#type: String,
	pub response: Response,
}

/// `response.output_item.*` 系イベントの共通 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutputItemEnvelope {
	#[serde(rename = "type", default)]
	pub r#type: String,
	pub output_index: u32,
	pub item: OutputItem,
}

/// `response.output_text.delta` の payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutputTextDeltaPayload {
	pub item_id: String,
	pub output_index: u32,
	pub content_index: u32,
	pub delta: String,
}

/// `response.output_text.done` の payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutputTextDonePayload {
	pub item_id: String,
	pub output_index: u32,
	pub content_index: u32,
	pub text: String,
}

/// `response.function_call_arguments.delta` の payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FunctionCallArgumentsDeltaPayload {
	pub item_id: String,
	pub output_index: u32,
	pub delta: String,
}

/// `response.function_call_arguments.done` の payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FunctionCallArgumentsDonePayload {
	pub item_id: String,
	pub output_index: u32,
	pub arguments: String,
}

/// Top-level `error` イベントの payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ErrorEnvelope {
	#[serde(rename = "type", default)]
	pub r#type: String,
	pub error: ErrorObject,
}
