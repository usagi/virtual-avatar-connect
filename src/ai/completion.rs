//! Responses API 呼び出しと tool-loop / gpt-5 系の空応答再試行 / オーバーフロー要約。
//!
//! χ-5 で Chat Completions 経路を全廃し、すべて [`crate::ai::openai_responses::ResponsesClient`]
//! 経由で走る。streaming 版は [`crate::ai::service`] 側が `create_stream` を直接呼ぶ
//! （tool-loop 併用の streaming は service 側で実装）。本 module は以下を担当する。
//!
//! - [`create_response`] — 1 発叩く薄いラッパ。
//! - [`create_response_resolve_tools`] — non-stream + tool loop。`openai_tools_json_path`
//!   が指定されていて streaming を諦めたケースで使う。
//! - [`retry_if_gpt5_empty_response`] — gpt-5 系が empty content を返したときの単発リトライ。
//! - [`summarize_overflow_turns`] — メモリ窓オーバーフロー要約（単独 Responses 呼び出し）。

use super::model_policy;
use super::tools::{self, ToolContext};
use crate::ai::openai_responses::types::input::{InputContent, InputItem};
use crate::ai::openai_responses::types::request::{CreateResponseRequest, ReasoningEffort};
use crate::ai::openai_responses::types::response::{OutputItem, Response};
use crate::ai::openai_responses::util::extract_output_text;
use crate::ai::openai_responses::{ResponsesClient, ResponsesClientError};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;

/// Responses API を 1 回叩く薄いラッパ（tool loop 無し）。
#[allow(dead_code)]
pub(crate) async fn create_response(
	client: &ResponsesClient,
	request: CreateResponseRequest,
) -> std::result::Result<Response, ResponsesClientError> {
	client.create(request).await
}

/// Responses API の non-stream + tool loop。
///
/// tool 宣言がある状態で streaming を諦めた場合（χ-5 時点では openai_tools_json + stream
/// の併用は service 側で実装したため、本関数は現在は未使用のバックアップ経路として残す）。
/// 各ラウンドで以下を繰り返す:
///
/// 1. `client.create(request)` で Responses を取得
/// 2. `response.output[]` から `FunctionCall` item を全部抽出
/// 3. function call が 1 件以上ある場合:
///    - それぞれ `tools::dispatch_tool_call` を呼んで `InputItem::FunctionCallOutput` を作成
///    - `FunctionCall` item と出力を次ラウンドの `input[]` に **積む**
///      （Responses API は client 側で全会話を再構築する前提）
///    - continue
/// 4. function call が無い場合: `extract_output_text` で assistant text を返す
#[allow(dead_code)]
pub(crate) async fn create_response_resolve_tools(
	client: &ResponsesClient,
	mut request: CreateResponseRequest,
	tool_ctx: &ToolContext,
) -> Result<String> {
	const MAX_TOOL_ROUNDS: usize = 8;
	let declared_local: HashSet<String> = request
		.tools
		.as_deref()
		.map(tools::locally_dispatched_tool_names)
		.unwrap_or_default();

	for _round in 0..MAX_TOOL_ROUNDS {
		let response = client.create(request.clone()).await.map_err(|e| anyhow::anyhow!("{e}"))?;

		let function_calls: Vec<OutputItem> = response
			.output
			.iter()
			.filter(|item| matches!(item, OutputItem::FunctionCall { .. }))
			.cloned()
			.collect();

		if function_calls.is_empty() {
			return extract_output_text(&response).context("AI からの応答はありましたが出力テキストがありませんでした。");
		}

		// 会話履歴（input）に今回の FunctionCall item 群をそのまま積む。Responses API は
		// `FunctionCall` と `FunctionCallOutput` が揃った状態で次の request を受け取る。
		for fc in &function_calls {
			if let OutputItem::FunctionCall {
				call_id, name, arguments, ..
			} = fc
			{
				request.input.push(InputItem::FunctionCall {
					call_id: call_id.clone(),
					name: name.clone(),
					arguments: arguments.clone(),
				});
			}
		}

		for fc in &function_calls {
			if let Some(view) = fc.as_function_call() {
				let out_item = tools::dispatch_tool_call(view, &declared_local, tool_ctx).await;
				request.input.push(out_item);
			}
		}
	}
	bail!("OpenAI ツール呼び出しのラウンド上限（{}）に達しました。", MAX_TOOL_ROUNDS);
}

/// gpt-5 系が empty な assistant text を返したときに、最小構成（system + user）で 1 回だけ再試行する。
///
/// Responses API に移行してからも gpt-5 系は reasoning token を消費した後 output_text が
/// 空になる事象が観測されるので、`reasoning.effort = Low` に落として `max_output_tokens` を
/// 圧縮した短い request で **最後の一声** を取りに行く。
///
/// 戻り値は再試行で得られた非空の応答テキスト（失敗 or 空なら `None`）。
pub(crate) async fn retry_if_gpt5_empty_response(
	client: &ResponsesClient,
	model_id: &str,
	latest_user: &str,
	orig_max_output_tokens: Option<u32>,
) -> Option<String> {
	if !model_policy::should_retry_on_empty_assistant(Some(model_id)) {
		return None;
	}
	let sys = InputItem::Message {
  role: "developer".to_string(),
  content: InputContent::Text(
   "You are a helpful assistant. Provide a concise final answer. If reasoning consumed tokens, output the answer briefly now. 日本語入力には日本語で返答して下さい。"
    .to_string(),
  ),
 };
	let user = InputItem::Message {
		role: "user".to_string(),
		content: InputContent::Text(latest_user.to_string()),
	};
	let mut req = CreateResponseRequest {
		model: model_id.to_string(),
		input: vec![sys, user],
		..Default::default()
	};
	let retry_cap: u32 = orig_max_output_tokens.map(|m| std::cmp::min(m, 128)).unwrap_or(128);
	// reasoning.effort は低めに倒して thinking token の消費を抑える。text.format も Text に。
	model_policy::apply_model_responses_options(&mut req, model_id, Some(retry_cap), Some(ReasoningEffort::Low));

	let res = client.create(req).await.ok()?;
	extract_output_text(&res).filter(|t| !t.trim().is_empty())
}

const OVERFLOW_SUMMARY_SYSTEM: &str = "\
あなたは会話ログの要約係です。与えられたログは、メモリ窓の上限で直近の対話から落とされた古い発話です。\
後続の対話と矛盾しないよう、事実・話題・必要なら固有名詞だけを残し、日本語で 2〜5 文以内に要約してください。\
メタ発言や挨拶は省き、内容のみ。";

/// メモリ窓から押し出された古い発話群を単発 Responses 呼び出しで要約する。
pub(crate) async fn summarize_overflow_turns(
	client: &ResponsesClient,
	model: &str,
	max_output_tokens: Option<u32>,
	user_payload: &str,
) -> Result<String> {
	let sys = InputItem::Message {
		role: "developer".to_string(),
		content: InputContent::Text(OVERFLOW_SUMMARY_SYSTEM.to_string()),
	};
	let user = InputItem::Message {
		role: "user".to_string(),
		content: InputContent::Text(user_payload.to_string()),
	};
	let mut req = CreateResponseRequest {
		model: model.to_string(),
		input: vec![sys, user],
		temperature: Some(0.2),
		..Default::default()
	};
	let cap = max_output_tokens.or(Some(256));
	model_policy::apply_model_responses_options(&mut req, model, cap, None);

	let res = client
		.create(req)
		.await
		.map_err(|e| anyhow::anyhow!("{e}"))
		.context("overflow summary: Responses create")?;
	extract_output_text(&res).context("overflow summary: 空の応答")
}
