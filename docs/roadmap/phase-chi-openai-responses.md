# Phase χ — OpenAI Responses API 全面移行

> VAC の OpenAI 連携を Chat Completions API から Responses API へ全面移行するフェーズ。
> 自前 `reqwest` + 自前 DTO（Option A）で実装し、将来 `un-discord-kaltsitpseudo` と
> shared crate 化できる配置にする。streaming / tool loop / gpt-5 retry / overflow summary /
> hot-reload を **1 PR / 9 commit** で full_parity 移植する。

---

## 1. Background

### 1.1 Chat Completions の制約

VAC は現在 [`async-openai = "0.34"`](../../Cargo.toml) の `chat-completion` feature を使い、[src/ai/service.rs](../../src/ai/service.rs) / [src/ai/completion.rs](../../src/ai/completion.rs) / [src/ai/tools.rs](../../src/ai/tools.rs) で以下を組んでいる:

- streaming（`client.chat().create_stream()`、`choices[0].delta.content` 1 種のみを処理）
- 8-round tool loop（`create_chat_completion_resolve_tools`）
- gpt-5 系の空応答再試行（`retry_if_gpt5_empty_content`）
- overflow summary / memory window / hot-reload

しかし Chat Completions API は設計上の限界を抱えている:

- **streaming と tool calling の共存が困難**: 現行 VAC は tool あり時は `effective_stream = false` で強制 non-stream に落としている
- **`reasoning.effort` を直接指定できない**（gpt-5 系の thinking token 制御が事実上不可）
- **`max_tokens` 命名が legacy**（`max_output_tokens` への移行が公式推奨）
- **built-in tools（web_search / file_search / code_interpreter / MCP）非対応**
- **`previous_response_id` 相当のサーバ会話状態が無い**

### 1.2 Responses API の優位点

- streaming と tools が同じ stream 上で共存（`response.function_call_arguments.delta` 等）
- `reasoning.effort` / `verbosity` を明示指定
- `max_output_tokens` 統一命名
- built-in tools へのアクセス経路
- サーバ側 conversation / compact 対応（本フェーズでは非採用、将来拡張）

### 1.3 un-discord-kaltsitpseudo `9dbd151` での先行事例

並行開発中の `usagi/un-discord-kaltsitpseudo` が `9dbd151 api -> Responses, and migrate for GPT5`（2026-04-23）で Responses に先行移行した。`crates/adapter_openai/src/lib.rs` の以下パターンを VAC へ輸入する:

- **endpoint**: `/v1/chat/completions` → `/v1/responses`
- **request DTO**:
  ```rust
  struct ResponseApiRequest {
      model: String,
      input: Vec<ResponseInputMessage>,
      temperature: f32,
      max_output_tokens: u32,
  }
  struct ResponseInputMessage { role: String, content: String }
  ```
- **response DTO**:
  ```rust
  struct ResponseApiResponse {
      output_text: Option<String>,       // convenience field
      output: Vec<ResponseOutputItem>,   // 正式 path
  }
  ```
- **`extract_output_text`**: `output_text` 優先、無ければ `output[].content[].text` を `\n` で join
- **`truncate_error_body`**（2048 chars 截断）と serde golden tests
- **config**: `openai_max_output_tokens` + `OPENAI_MAX_OUTPUT_TOKENS`、旧 `openai_max_tokens` / `OPENAI_MAX_TOKENS` は fallback

un-discord にはない機能（VAC で新規実装する必要あり）:

- streaming SSE パーサ
- tool-call streaming（`response.function_call_arguments.delta/done`）
- function-call 向け input item（`FunctionCallOutputItemParam` 相当）の reconstruction
- gpt-5 系の `reasoning.effort` パラメタ
- hot-reload 対応の request テンプレート構造

### 1.4 crate 選定の判断過程

検討した 3 択:

- **Option A**: `reqwest` 直叩き + 自前 DTO（un-discord と同じ思想）
- **Option B**: `async-openai` 0.34 → 0.35 にバンプして `client.responses()` を使う
- **Option C**: ハイブリッド（非 stream は SDK、stream は reqwest）

**Option A を採用**。決定要因:

1. un-discord が将来 streaming/tools 追加方向なので、**shared crate 化の下準備**としてスタイルを揃える価値がある
2. OpenAI spec 追従のリードタイムを 0 にしたい（`zero_lag_needed`）。SDK 追従は通常 1 ヶ月ペース
3. Files / Fine-tuning API は `async-openai = "0.34"` を温存（Chat Completions / Responses と直交）

### 1.5 χ の目標

- Chat Completions 経由を完全撤去し、Responses API に全面切替（big-bang）
- streaming と tools が同一 stream 上で共存する構造に切替
- gpt-5 系の `reasoning.effort` を runtime 指定可能化
- `openai_max_output_tokens` 命名に移行、旧キーは fallback
- `src/ai/openai_responses/` を **shared crate 切り出し可能**な境界で実装
- docs / CHANGELOG / conf.example を 1 PR で同時更新

---

## 2. Goals / Non-Goals

### 2.1 Goals

- `/v1/responses` エンドポイントでの non-stream / streaming 両対応
- 8-round tool loop の全面 Responses 化（streaming tool も視野）
- gpt-5 系の `reasoning.effort` / empty retry を Responses 流儀で再実装
- `openai_max_output_tokens` 導入 + 旧命名の後方互換
- `src/ai/openai_responses/` の `crate::*` 非依存（shared crate 切り出し準備）
- 既存 overflow summary / memory window / hot-reload 資産の温存

### 2.2 Non-Goals（χ では扱わない）

- **`previous_response_id` / `conversation` / `compact` API 採用**: VAC の `ChannelDatum` / memory window 資産と衝突するため、Phase ψ+ で別途検討
- **built-in tools（`web_search_preview` / `file_search` / `code_interpreter` / MCP tool）**: 個別対応は将来フェーズ
- **shared crate (`vac-openai-responses`) 化そのもの**: χ では VAC 内蔵のまま、境界設計だけ整える
- **async-openai の 0.35 バンプ**: Files / Fine-tuning は 0.34 のまま温存
- **feature flag による Chat Completions 並走**: 維持コストが高いため big-bang で切替

---

## 3. Architecture

### 3.1 新モジュール `src/ai/openai_responses/`

```
src/ai/openai_responses/
  mod.rs            -- 公開 API facade（ResponsesClient / error type）
  types/
    mod.rs
    request.rs      -- CreateResponseRequest, Reasoning, ToolChoice, ResponseFormat, Tool
    response.rs     -- Response, OutputItem, Usage, ErrorObject (non-stream)
    input.rs        -- InputItem enum: { Message, FunctionCall, FunctionCallOutput, Reasoning }
    stream.rs       -- StreamEvent enum (7 variants)
  client.rs         -- ResponsesClient { http: Arc<reqwest::Client>, cfg: Cfg }
                       -- `create(req)` / `create_stream(req)` を提供
  sse.rs            -- SSE パーサ（data: / [DONE] / event: 対応、イベント種別 dispatch）
  util.rs           -- extract_output_text, truncate_error_body
  tests/
    golden.rs       -- serde_json ゴールデン（OpenAI 公式 docs サンプル payload で）
    stream.rs       -- モック SSE バイト列 → StreamEvent 列検証
```

### 3.2 境界原則（shared crate 切り出し準備）

- `src/ai/openai_responses/` 以下は **`crate::*` への依存を作らない**
- `SharedState` / `ChannelDatum` / `AiRuntime` / `ConfLayout` 等を import しない
- 入出力は自前 DTO + `String` / `Vec<u8>` / `serde_json::Value` で完結
- log は `log` crate 経由（VAC 全体のファサード）で OK、`crate::logger::*` は NG
- エラー型は `thiserror::Error` ベースの自己完結 enum（`anyhow` への変換はモジュール境界で行う）

### 3.3 依存変更（`Cargo.toml`）

- `async-openai = "0.34"` 維持（features を `["finetuning", "file"]` に削減、`"chat-completion"` 削除）
- 新規追加: `eventsource-stream = "0.2"`（SSE バッファリング）
- 既存の `reqwest` / `futures` / `tokio` / `serde` / `serde_json` / `thiserror` は流用

---

## 4. Responses API Type Mapping

Chat Completions → Responses の型対応表（VAC で書き換わる箇所）:

| Chat Completions 型 | Responses（自前 DTO） | 備考 |
|---|---|---|
| `CreateChatCompletionRequest` | `CreateResponseRequest` | `messages` → `input`、`max_tokens` → `max_output_tokens` |
| `ChatCompletionRequestMessage::{System,User,Assistant}` | `InputItem::Message { role, content }` | role は `"system"` / `"user"` / `"assistant"` / `"developer"` |
| `ChatCompletionRequestToolMessage` | `InputItem::FunctionCallOutput { call_id, output }` | tool 応答の戻し方が変わる |
| `ChatCompletionTools::Function` | `Tool::Function { name, description, parameters, strict }` | JSON schema 本体は同じ |
| `ChatCompletionTools::Custom` | `Tool::Custom { name, description, input_schema }` | 同上 |
| `ChatCompletionToolChoiceOption::Mode(Auto/None/Required)` | `ToolChoice::Mode(Auto/None/Required)` | 名前揃えるだけ |
| `CreateChatCompletionResponse` | `Response` | `choices[0].message.content` → `output_text` |
| `choices[0].message.tool_calls` | `output[].as_function_call()` | output item 配列を走査して function_call を抽出 |
| `CreateChatCompletionStreamResponse`（SSE chunk） | `StreamEvent` enum | 変種が 1 → 7 に増える（§5 参照） |
| `ResponseFormat::JsonSchema` | `ResponseFormat::JsonSchema { name, schema, strict }` | 構造は同等、命名差 |
| （なし） | `Reasoning { effort, summary }` | gpt-5 系 thinking token 制御（新規） |

### 4.1 `Response` structure（抜粋）

```rust
pub struct Response {
    pub id: String,
    pub model: String,
    pub output_text: Option<String>,     // convenience
    pub output: Vec<OutputItem>,
    pub usage: Option<Usage>,
    pub status: Option<ResponseStatus>,  // completed / incomplete / failed
    pub error: Option<ErrorObject>,
}

pub enum OutputItem {
    Message { id: String, role: String, content: Vec<MessageContent> },
    FunctionCall { id: String, call_id: String, name: String, arguments: String },
    Reasoning { id: String, summary: Option<String> },
    // ... 将来拡張
}

pub enum MessageContent {
    OutputText { text: String, annotations: Option<Vec<Value>> },
    Refusal { refusal: String },
}
```

### 4.2 `CreateResponseRequest` structure（抜粋）

```rust
pub struct CreateResponseRequest {
    pub model: String,
    pub input: Vec<InputItem>,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub reasoning: Option<Reasoning>,      // gpt-5 系のみ有効
    pub response_format: Option<ResponseFormat>,
    pub tools: Option<Vec<Tool>>,
    pub tool_choice: Option<ToolChoice>,
    pub parallel_tool_calls: Option<bool>,
    pub stream: Option<bool>,
    pub store: Option<bool>,               // default true、false で履歴非保存
    pub metadata: Option<HashMap<String, String>>,
}
```

### 4.2b `Tool` structure（χ-3 で拡張済み）

χ-3 で hosted tools を含む 5 variant に拡張した。ユーザ関数は `Function` / `Custom`、OpenAI 側 hosted は `WebSearch` / `FileSearch` / `CodeInterpreter`。`openai_tools_json` は Chat Completions の `{type:"function",function:{...}}` と Responses の `{type:"function",name:...}` の両方を受け入れる（`tools::parse_tools_json` で自動判定）。

```rust
pub enum Tool {
    Function { name, description?, parameters, strict? },
    Custom   { name, description?, format? },
    WebSearch       { user_location?, search_context_size? },
    FileSearch      { vector_store_ids, max_num_results?, filters? },
    CodeInterpreter { container? },
}

impl Tool {
    pub fn name(&self) -> &str;            // hosted は "web_search" 等を返す
    pub fn is_locally_dispatched(&self) -> bool;  // hosted は false
}
```

hosted tools は VAC では実行せず OpenAI 側で完結し、`output[]` に `web_search_call` / `file_search_call` / `code_interpreter_call` 等の item として混入する（χ-2 の `StreamEvent::Other`、χ-5 で個別 handling 追加予定）。χ-3 時点では **Chat Completions pipeline に載らない**ため、`tools::tools_to_chat_completions` が除外して warn ログを出す。χ-5 の Responses 全面移行で有効化される。

### 4.2c Request assembly（χ-4 で追加）

χ-4 で Responses API 用の request / input 組み立て関数を **追加** した（Chat Completions 用は残置。χ-5 で旧経路を剥がす）。

| Chat Completions 版（既存） | Responses 版（χ-4 で追加） |
|---|---|
| `context::assemble_openai_chat_messages` → `Vec<ChatCompletionRequestMessage>` | `context::assemble_openai_responses_input` → `Vec<InputItem>` |
| `model_policy::apply_model_chat_options(builder, model, max_tokens: u16)` | `model_policy::apply_model_responses_options(request, model, max_output_tokens: u32, reasoning_effort)` |
| `service::make_request_template(persona) -> CreateChatCompletionRequest` | `reload::make_responses_request_template(persona, max_output_tokens, reasoning_effort, store) -> CreateResponseRequest` |

主な挙動差:

- **Instruction role**: gpt-5 系では `custom_instructions` / `system_instructions_extra` を `role: "developer"` で投入する（他 system 扱いは従来通り `role: "system"`）。gpt-5 は developer role を特別扱いするため。
- **Default instruction insertion**: gpt-5 系で instruction が 1 本も無いときだけ先頭に既定文を insert する挙動は Chat Completions 版と同じ。Responses 版は `system` / `developer` のどちらかを見つけた時点で skip する。
- **`max_tokens` → `max_output_tokens`**: Responses は `u32`。`u16` の `persona.max_tokens` を legacy fallback として受理（χ-6 で `openai_max_output_tokens` が正式 conf キーに昇格したら fallback を削除）。
- **`reasoning.effort`**: gpt-5 系のみ。非 gpt-5 系で指定された場合は warn して落とす。
- **`text.format`**: gpt-5 系でのみ既定 `Text` を埋めて structured output 誤起動を防ぐ（旧 `ResponseFormat::Text` の Responses 等価）。
- **`store`**: `None` を渡すと API 既定（true）、`Some(false)` で server-side 履歴を無効化する。VAC は client 側で input を構築するので χ-5 以降は `Some(false)` を既定にする予定。

### 4.3 `InputItem` structure

```rust
pub enum InputItem {
    Message { role: String, content: InputContent },
    FunctionCall { call_id: String, name: String, arguments: String },
    FunctionCallOutput { call_id: String, output: String },
    // Reasoning は response 側にしか出ず、input 側には含めない
}

pub enum InputContent {
    Text(String),
    Parts(Vec<InputContentPart>),
}

pub enum InputContentPart {
    InputText { text: String },
    InputImage { image_url: String, detail: Option<String> },
    // 将来 InputFile など
}
```

---

## 5. Streaming Event Design

Chat Completions SSE は `choices[0].delta.content` の 1 種だけだが、Responses SSE は 40+ のイベント種別がある。VAC が実際に処理するのは以下 **7 種** のみ:

| イベント | VAC 処理 |
|---|---|
| `response.created` | `response_id` を記録、ログ出力 |
| `response.in_progress` | no-op（デバッグログのみ） |
| `response.output_item.added` | 新 item 開始を検知（message / function_call / reasoning を区別） |
| `response.output_text.delta` | **Chat の `delta.content` 相当**。`content.push_str(&delta)` してストリーミング表示 |
| `response.function_call_arguments.delta` | `call_id` ごとに arguments を accumulator へ push |
| `response.output_item.done` | function_call item 完了時に accumulator から 1 本の JSON として取り出す |
| `response.completed` | 最終 usage / status を記録、stream 終了 |
| `response.failed` / `response.error` | エラー扱いで `bail!`、`eprint_openai_usage_hint_if_needed` 連携 |

他のイベント（`response.content_part.added` / `.done`、`response.reasoning.delta` 等）は受信しても no-op でドロップ。

### 5.1 `StreamEvent` enum

```rust
pub enum StreamEvent {
    Created { response_id: String, model: String },
    InProgress,
    OutputItemAdded {
        index: u32,
        item_kind: ItemKind,   // Message / FunctionCall / Reasoning
        call_id: Option<String>,
        name: Option<String>,
    },
    OutputTextDelta { item_id: String, delta: String },
    FunctionCallArgumentsDelta { call_id: String, delta: String },
    OutputItemDone { index: u32, item: OutputItem },
    Completed { response_id: String, usage: Option<Usage> },
    Failed { error: ErrorObject },
    Other { raw_type: String },   // 未知イベントは raw_type のみ保持してドロップ
}
```

### 5.2 SSE パーサ

`eventsource-stream` crate で `reqwest::Response` の bytes stream を SSE イベント単位に割る。各イベントは:

```
event: response.output_text.delta
data: {"type":"response.output_text.delta","item_id":"msg_abc","delta":"Hello"}

```

形式。`eventsource_stream::Eventsource` が `event` field と `data` field を分けて返すので:

1. `data` を `serde_json::from_str::<serde_json::Value>()` で parse
2. `type` field で分岐（既知 7 種 → `StreamEvent`、未知 → `StreamEvent::Other`）
3. `[DONE]` 受信で stream 終了

### 5.3 function_call streaming の組み立て

1. `response.output_item.added` で `ItemKind::FunctionCall { call_id, name }` が届く
2. `response.function_call_arguments.delta` が複数回 `call_id` で届く
3. `response.output_item.done` で該当 `call_id` の `arguments` 全文が確定

途中で JSON parse を試みない（不完全 JSON が流れる）。`.done` を受け取ってから一括 `serde_json::from_str` する。

---

## 6. Tool Loop Design

現行 [src/ai/completion.rs](../../src/ai/completion.rs) の `create_chat_completion_resolve_tools`（8-round loop）を Responses 版に書き換える:

```rust
pub(crate) async fn create_response_resolve_tools(
    client: &ResponsesClient,
    mut request: CreateResponseRequest,
    tool_ctx: &ToolContext,
) -> Result<String> {
    let declared = request.tools.as_deref()
        .map(tools::declared_tool_names).unwrap_or_default();
    const MAX_TOOL_ROUNDS: usize = 8;
    for _ in 0..MAX_TOOL_ROUNDS {
        let response = client.create(request.clone()).await?;
        let function_calls = response.output.iter()
            .filter_map(|item| item.as_function_call())
            .collect::<Vec<_>>();
        if function_calls.is_empty() {
            return Ok(extract_output_text(&response).unwrap_or_default());
        }
        for fc in &function_calls {
            if let Some(out) = tools::dispatch_function_call(fc, &declared, tool_ctx).await {
                request.input.push(InputItem::FunctionCallOutput {
                    call_id: fc.call_id.clone(),
                    output: out,
                });
            }
        }
    }
    bail!("OpenAI ツール呼び出しのラウンド上限（{}）に達しました。", MAX_TOOL_ROUNDS);
}
```

### 6.1 `FunctionCallOutput` の連結

Chat Completions では `ChatCompletionRequestToolMessage { tool_call_id, content }` を `messages` に push していた。Responses では `InputItem::FunctionCallOutput { call_id, output }` を `input` に push する。

**重要**: Chat Completions の `ChatCompletionRequestAssistantMessage { tool_calls: [...] }` に相当する「assistant が function_call を呼んだ履歴」は、Responses では **prior response の output** がそのまま server 側で next request の input として参照される運用が推奨されているが、本フェーズでは `previous_response_id` を使わず **client 側で全 input を構築する** 方針のため、function_call 自体も `InputItem::FunctionCall { call_id, name, arguments }` として input に積む必要がある。

### 6.2 streaming tool-loop の扱い

- streaming 中に function_call が出たら、stream を最後まで読んで `FunctionCall { call_id, name, arguments }` を組み立て、次のラウンドで input に積んで再 request
- 2 ラウンド目以降も streaming を維持できる（Chat Completions では不可だった）
- **本フェーズでの実装**: まず tool あり時も streaming 維持する方向で書き、text delta が来る前に function_call が確定したら即 dispatch → 次 round という流れ。複雑性が高ければ χ-5 の中で non-stream fallback を残す選択肢も許容（Open Question §11）

---

## 7. Configuration Changes

### 7.1 `openai_max_output_tokens` 導入

- conf キー追加: `[[ai.personas]] openai_max_output_tokens = 512`（optional）
- env 追加: `OPENAI_MAX_OUTPUT_TOKENS`
- 旧キー `openai_max_tokens` / `OPENAI_MAX_TOKENS` は warn 無しで fallback 受理（un-discord と同じ方針）
- 両方指定時は新キー優先

### 7.2 `openai_reasoning_effort` 導入（新規、optional）

gpt-5 系のみ有効:

```toml
[[ai.personas]]
openai_model = "gpt-5-mini"
openai_reasoning_effort = "medium"  # low / medium / high
```

env 未指定時は `None` を送り OpenAI デフォルトに任せる。

### 7.3 `openai_store` 導入（新規、optional、default true）

サーバ側に会話履歴を保存するか。VAC は client 側で全 input を構築するため、**`false` を推奨**（将来 `previous_response_id` を使う Phase ψ+ で再検討）。

### 7.4 旧キーの扱い

- `openai_tool_choice`（`"auto"` / `"none"` / `"required"`）: そのまま維持
- `openai_parallel_tool_calls`: そのまま維持
- `openai_tools_json_path`: そのまま維持（JSON スキーマ自体は互換）
- `openai_stream`: そのまま維持（Responses でも意味は同じ）

---

## 8. Sub-phases & Commit Plan

| # | prefix | 内容 | 対象ファイル |
|---|---|---|---|
| χ-0 | `χ-0 docs:` | roadmap.md / architecture.md / phase-chi-openai-responses.md 新設 + cross-link | `docs/roadmap.md` / `docs/architecture.md` / `docs/roadmap/phase-chi-openai-responses.md` / `docs/roadmap/backlog-nodes.md` / `docs/roadmap/phase-phi-*.md` |
| χ-1 | `χ-1 feat(ai/responses):` | scaffold + Cargo deps 調整 + non-stream DTO types + `create` + golden tests | `Cargo.toml` / `src/ai/mod.rs` / `src/ai/openai_responses/mod.rs` / `types/*.rs` / `client.rs` / `util.rs` / `tests/golden.rs` |
| χ-2 | `χ-2 feat(ai/responses):` | `StreamEvent` enum + SSE parser + モック stream tests | `src/ai/openai_responses/types/stream.rs` / `sse.rs` / `client.rs`（`create_stream` 追加） / `tests/stream.rs` |
| χ-3 | `χ-3 refactor(ai/tools):` | Tool 定義 + `dispatch_tool_call` を Responses 型に移行 | `src/ai/tools.rs` |
| χ-4 | `χ-4 refactor(ai/context,reload,model_policy):` | request assembly を `CreateResponseRequest` に置換 | `src/ai/context.rs` / `src/ai/reload.rs` / `src/ai/model_policy.rs` |
| χ-5 ✅ | `χ-5 refactor(ai/service,completion):` | streaming / tool-loop / gpt-5 retry を Responses に全面移行 | `src/ai/service.rs` / `src/ai/completion.rs` / `src/ai/context.rs` / `src/ai/model_policy.rs` / `src/ai/tools.rs` |
| χ-6 | `χ-6 refactor(conf):` | `openai_max_output_tokens` + レガシーフォールバック + `openai_reasoning_effort` | `src/ai/config.rs` / `src/conf/mod.rs` |
| χ-7 | `χ-7 docs:` | manual / conf.example / CHANGELOG / roadmap.md χ tick | `docs/manual/conf-reference.md` / `docs/manual/tutorials/openai-persona.md` / `conf.example*.toml` / `CHANGELOG.md` / `docs/roadmap.md` |
| χ-8.0 | `χ-8.0 fix(flowgraph/docs):` | `node_catalog_md_up_to_date` を line-ending 正規化で CRLF 環境でも通す（χ-5 以前からの pre-existing bug、§9.5 参照） | `src/flowgraph/docs.rs` |
| χ-8 | `χ-8 test:` | 全体テスト + 実機スモーク + 必要ならリリースノート微修正 | テスト実行 + CHANGELOG 調整のみ（コード変更は基本 0） |

計 10 commit / 1 PR。χ-0 はドキュメント先行で、χ-1..χ-8 が順序依存の実装。χ-8.0 は χ-8 本体の前提（`cargo test --lib` 全 pass を実効的に保証するため χ-8 の冒頭で commit）。

---

## 9. Test Plan

### 9.1 Unit (Rust `cargo test`)

- `src/ai/openai_responses/tests/golden.rs`:
  - `extract_output_text` が `output_text` 優先を確認（un-discord の 2 テストを輸入）
  - `CreateResponseRequest` → JSON serialize が期待形
  - `Response` の典型 payload（message / function_call 混在）をデシリアライズできる
  - `ErrorObject` のフィールドが揃う
- `src/ai/openai_responses/tests/stream.rs`:
  - 公式 docs の SSE サンプルバイト列 → `StreamEvent` 列が期待通り
  - function_call streaming: added → delta × N → done で arguments が完全 JSON になる
  - 未知イベントは `StreamEvent::Other` に落ちて後続をブロックしない
  - `[DONE]` で stream 終了、エラーを投げない
  - `response.failed` で `StreamEvent::Failed`
- `src/ai/tools.rs`（既存）:
  - `Tool::Function` の JSON 表現が OpenAI schema を満たす
  - `dispatch_function_call` の戻りが `FunctionCallOutput::output` に正しく入る
- `src/ai/completion.rs`:
  - tool loop 8 round 上限の挙動
  - retry_if_gpt5_empty_response（旧 retry_if_gpt5_empty_content 相当）

### 9.2 Integration

- `cargo test --all` 全通過
  - 既知の `flowgraph::docs::docs_tests::node_catalog_md_up_to_date` は Windows の `core.autocrlf=true` + CRLF/LF 不一致で fail する **pre-existing bug**。χ-8 の最初（χ-8.0）で line-ending 正規化パッチを当てて解消する（§9.5 参照）
- `cd gui && npm run check` 0 errors
- `cd gui && npm run build` 成功

### 9.3 Manual smoke

- `gpt-4.1-mini` で non-stream 応答（シンプル persona）
- `gpt-4.1-mini` で streaming 応答
- `gpt-4.1-mini` で tool 付き streaming（`vac_ping` / `vac_emit_effect`）
- `gpt-5-mini` で `reasoning.effort = "low"` / `"medium"` / `"high"` の挙動差
- `gpt-5-mini` で空応答 retry が発動するケース（意図的に instruction を lean にする）
- hot-reload で persona の `instructions` を変更 → atomic swap で反映
- overflow summary を発動させる（長い履歴で memory window が溢れる）

### 9.4 CI

- `.github/workflows/*.yml` は既存のものを流用（fmt / clippy / build / test）
- 新しい外部依存 `eventsource-stream` の license（MIT/Apache-2.0）を `CHANGELOG` に記載

### 9.5 Pre-existing test bug: `node_catalog_md_up_to_date` の CRLF/LF 対処

`src/flowgraph/docs.rs` の `docs_tests::node_catalog_md_up_to_date` は、`render_node_catalog_md(&default_registry())` が返す文字列（LF 改行）と、`std::fs::read_to_string(docs/manual/node-catalog.md)`（Windows の `core.autocrlf=true` で checkout すると CRLF 改行）を **バイト比較** しているため、Windows 環境で必ず fail する既知のバグ。

`.gitattributes` で `eol=lf` を強制する手もあるが、既存 clone に対して破壊的（`git add --renormalize .` が必要）なので、テスト側で line-ending を正規化するアプローチを取る。

#### χ-8.0 で当てる最小パッチ

```rust
// src/flowgraph/docs.rs :: docs_tests::node_catalog_md_up_to_date
let actual = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!(/* ... */));
let actual_norm = actual.replace("\r\n", "\n");
let expected_norm = expected.replace("\r\n", "\n");

if actual_norm != expected_norm {
  let diff_hint = "BLESS_NODE_CATALOG=1 で再生成してください。";
  panic!("docs/manual/node-catalog.md が registry と不一致。{diff_hint}");
}
```

`BLESS_NODE_CATALOG=1` 経路も `expected.replace("\r\n", "\n")` を書き出して LF 固定にする（`std::fs::write` は byte を触らないので、`expected` が LF なら LF のまま書かれる。明示的 normalize は防御）。

#### χ-8.0 の commit 方針

- Commit prefix: `χ-8.0 fix(flowgraph/docs):`
- 対象: `src/flowgraph/docs.rs` のみ（`docs/manual/node-catalog.md` は触らない）
- テスト: Windows / Linux の両方で `cargo test --lib flowgraph::docs::docs_tests` が pass
- この fix は χ-5 完了時点での `cargo test --lib` 全 pass を実効的に保証するための前提作業で、Responses API 移行とは独立した範囲修正

---

## 10. Migration / Backward Compat

### 10.1 既存 conf への影響

- `openai_max_tokens` / `OPENAI_MAX_TOKENS` は warn なしで fallback
- `openai_model` の値はそのまま通す（`gpt-4.1-mini` も Responses で使用可能）
- `openai_stream` は意味同じ
- `openai_tools_json_path` は同じ JSON schema で動く（`parameters` 構造が両 API で共通）

### 10.2 旧 Chat Completions コード残滓

- `async-openai` の `chat-completion` feature を削除するので、`use async_openai::types::chat::*` は全部エラーになる
- これを **機械的に発見するため** χ-1..χ-5 で順に削除し、χ-5 完了時には Chat 型への import は 0 件を確認

### 10.3 ロールバック計画

- 1 PR だが 9 commit に刻むので、問題発生時はその commit 単体を revert 可能
- 最悪 χ-5 以前に戻しても runtime が動く（conf 変更は χ-6、docs は χ-7 のため、χ-5 までで runtime 本体は閉じている）

---

## 11. Open Questions / Future Extensions

- **Streaming tool-loop の複雑性**: χ-5 実装中に streaming + tool 共存で race / partial JSON 問題が出た場合、non-stream fallback を fall-through で残す選択肢も許容。判断は実装時に commit log に記録
- **`previous_response_id` 採用**: サーバ側会話状態を使うと VAC の memory window / overflow summary 資産と重複する。Phase ψ+ で評価
- **`conversation` / `compact` API**: 長期セッション向け。VAC の用途（リアルタイム配信）では必要性が低いが、AI Persona の持続 persona を強化する文脈で再検討候補
- **built-in tools（`web_search_preview` / `file_search` / `code_interpreter` / MCP tool）**: 個別フェーズで採用可否を議論。MCP tool は [`src/web_interface/control/`](../../src/web_interface/control/) と組み合わせて VAC 自身を MCP server 化する方向も考えられる
- **`vac-openai-responses` shared crate 化**: un-discord-kaltsitpseudo が streaming / tools 実装したタイミングで切り出し候補。Phase ω 想定
- **Structured Outputs (JSON mode)**: 現行 `ResponseFormat::JsonSchema` 相当を Responses でも維持。OpenAI の JSON schema extension に追従

---

## 12. References

- 参考実装: [`usagi/un-discord-kaltsitpseudo@9dbd151`](https://github.com/usagi/un-discord-kaltsitpseudo/commit/9dbd15138fc496794f56b528570117d71dccde31) — `api -> Responses, and migrate for GPT5`
- 既存 VAC AI 実装:
  - [`src/ai/service.rs`](../../src/ai/service.rs) — event loop / streaming
  - [`src/ai/completion.rs`](../../src/ai/completion.rs) — tool loop / gpt-5 retry
  - [`src/ai/tools.rs`](../../src/ai/tools.rs) — function calling
  - [`src/ai/context.rs`](../../src/ai/context.rs) — message assembly
  - [`src/ai/model_policy.rs`](../../src/ai/model_policy.rs) — model 別戦略
- OpenAI 公式:
  - [Responses API reference](https://platform.openai.com/docs/api-reference/responses)
  - [Responses API guide (streaming / reasoning / tools)](https://platform.openai.com/docs/guides/responses)
- 関連 crate:
  - [`eventsource-stream`](https://docs.rs/eventsource-stream/) — SSE バッファリング
  - [`async-openai` 0.35](https://docs.rs/async-openai/0.35.0/async_openai/types/responses/index.html) — 採用しないが参照（型設計の cross-check に使う）
- フェーズ管理:
  - [`docs/roadmap.md`](../roadmap.md) — 全フェーズ tick
  - [`docs/architecture.md`](../architecture.md) — Commit Granularity Rule
  - [`docs/roadmap/v2-merge-pr.md`](v2-merge-pr.md) — v2 → main 合流計画（参考）
