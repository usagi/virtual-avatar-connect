# Phase ψ-α — Encrypted Reasoning Passthrough

> Phase χ で全面移行した OpenAI Responses API の上で、gpt-5 系 reasoning の
> 効率を上げる狭い範囲の最適化。`store: false` を維持したまま、tool loop の
> round 間で `reasoning.encrypted_content` blob を client 側で持ち回ることで、
> reasoning tokens の重複課金を抑える。

---

## 0. Status

- 起点: Phase χ 完了（commit `e23387b` 含む）。
- 先行検討: [`phase-chi-openai-responses.md`](phase-chi-openai-responses.md) §11.2 で設計メモを残した。本フェーズでそれを実装に昇格する。
- 並行: Phase ν（GUI E2E testing with Playwright）。コードの触る場所が違うので衝突しない。

---

## 1. Background

### 1.1 gpt-5 系の reasoning token コストと `store: false` のトレードオフ

gpt-5 系モデルは response 生成前に **reasoning tokens** を内部で消費する。tool loop のように 1 request で結論が出ず、function_call → tool_result → 追加 reasoning を繰り返す場合、round ごとに reasoning を作り直すと:

- reasoning tokens の重複課金（output tokens として billing される）
- round 間の思考コヒーレンスが切れる（OpenAI 公式ドキュメントが推奨する "pass back reasoning items" と真逆）
- p95 latency 悪化（各 round で "考え直し" が発生）

一方 VAC は χ-6 以降 `openai_store = false`（既定）を採用しており、OpenAI サーバ側に会話履歴を保存しない設計にしている。理由は [`phase-chi-openai-responses.md`](phase-chi-openai-responses.md) §11.1 参照。

### 1.2 `include: ["reasoning.encrypted_content"]` による中間解

`store: false` のまま reasoning state を持ち回る公式手段。仕組み:

1. request に `"include": ["reasoning.encrypted_content"]` を足す
2. response の `output[]` に含まれる `Reasoning` item が `encrypted_content: "<base64 blob>"` を持つ
3. 次の request の `input[]` に、その Reasoning item を（FunctionCall / FunctionCallOutput と共に）詰め直す
4. OpenAI サーバ側 state は不要（`store: false` のまま）

公式ドキュメントの要点（2026-04 時点、[reasoning.md](https://platform.openai.com/docs/guides/reasoning#encrypted-reasoning-items)）:

> When using the Responses API in a stateless mode (either with `store` set to `false`, or when an organization is enrolled in zero data retention), you must still retain reasoning items across conversation turns using the techniques described above.
>
> Our systems will smartly ignore any reasoning items that aren't relevant to your functions, and only retain those in context that are relevant.

→ VAC 側で余計な分岐判断は不要、過去の Reasoning item をそのまま積み直すだけでよい。

### 1.3 VAC における適用範囲の限定（重要）

**1 回の `react()` 呼び出し内の tool loop round 間のみ** に限定する。

- 複数 `react()` 呼び出しを跨ぐ持ち回りは **やらない**。理由:
  - hot-reload（conf.toml 変更）で persona instructions / memory window / tools が書き換わる
  - overflow summary（memory window trim）で input shape が再構築される
  - 結果として blob が参照する reasoning のコヒーレンスが保てない
- 1 `react()` 内の tool loop round 間は context が安定しているので blob を安全に持ち回れる
- 改修範囲は `drive_responses_tool_loop`（[`src/ai/service.rs`](../../src/ai/service.rs)）の round-over-round に閉じる（narrow scope）

---

## 2. Acceptance Criteria

### 2.1 コード

- [ ] `CreateResponseRequest.include: Option<Vec<String>>` を追加
- [ ] `InputItem::Reasoning { id, encrypted_content?, summary? }` を追加（output → input round-trip 用）
- [ ] `OutputItem::Reasoning.encrypted_content: Option<String>` を追加
- [ ] `openai_reasoning_encrypted_passthrough: Option<bool>` conf key（既定 `true`、opt-out 可能）
- [ ] `drive_responses_tool_loop` が以下を満たす:
  - gpt-5 系 + passthrough 有効時のみ、初回 request に `include: ["reasoning.encrypted_content"]` を付加
  - 各 round 終了時、response.output から Reasoning item を抽出（encrypted_content 有無は問わない）
  - 次ラウンド input に Reasoning → FunctionCall → FunctionCallOutput の順で積む（OpenAI の "pass all items between last user message and your function call output" に従う）
  - 壊れた/欠落 blob は `log::warn!` + blob 無しで継続（現行動作に degrade）
  - 非 gpt-5 モデルでは no-op（include も付かない、Reasoning item の pass-through もしない）

### 2.2 テスト

- [ ] golden test: `CreateResponseRequest { include: Some(vec!["reasoning.encrypted_content".into()]), ... }` のシリアライズ結果が仕様通り
- [ ] golden test: `OutputItem::Reasoning { encrypted_content: Some("..."), ... }` のデシリアライズが通る
- [ ] golden test: `InputItem::Reasoning { id: "rs_...", encrypted_content: Some("..."), ... }` のシリアライズが通る
- [ ] unit test (service.rs): gpt-5 + passthrough 有効で tool loop 2 round 回した時、2 round 目の request.input[] に前ラウンドの Reasoning item が含まれる
- [ ] unit test (service.rs): 非 gpt-5（例: gpt-4o-mini）では include も Reasoning pass-through も no-op

### 2.3 ドキュメント

- [ ] `CHANGELOG.md` に ψ-α エントリを追加
- [ ] `docs/manual/conf-reference.md` に `openai_reasoning_encrypted_passthrough` を追記
- [ ] `conf.example-openai-chat.toml` の header コメントに ψ-α 対応を一言追加
- [ ] `docs/roadmap.md` の ψ-α サブフェーズ tick を全緑化
- [ ] 実機計測: gpt-5-mini + tool loop の reasoning tokens / p95 latency を passthrough ON/OFF で比較し CHANGELOG に numbers 記録

### 2.4 互換性

- 既存 conf で `openai_reasoning_encrypted_passthrough` 未指定 → 既定 `true` で自動有効化
- 非 gpt-5 モデルでは完全に no-op（`gpt-4o-mini` 等を使っている既存利用者にはゼロ影響）
- 万一 OpenAI 側がフィールド命名を変えた場合に備え、`include` / blob 欠落は全て warn + fallback

---

## 3. Sub-phase Breakdown

| sub | 内容 | 対象ファイル |
|---|---|---|
| ψ-α-0 | docs: phase doc 新設 + roadmap Active 移行 | `docs/roadmap.md` / `docs/roadmap/phase-psi-alpha-encrypted-reasoning.md`（本書） |
| ψ-α-1 | feat(ai/responses): DTO 拡張 + golden tests | `src/ai/openai_responses/types/request.rs` / `.../types/input.rs` / `.../types/response.rs` / `.../tests.rs` |
| ψ-α-2 | feat(ai/service,config): tool loop round で reasoning pass-through + conf key | `src/ai/service.rs` / `src/ai/config.rs` / `src/ai/model_policy.rs` |
| ψ-α-3 | docs: CHANGELOG / conf-reference / conf.example + 実機計測 + roadmap tick | user-facing docs |

---

## 4. Design Notes

### 4.1 DTO 追加の具体形

```rust
// src/ai/openai_responses/types/request.rs
pub struct CreateResponseRequest {
    // ... existing fields ...
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub include: Option<Vec<String>>,
}
```

```rust
// src/ai/openai_responses/types/input.rs
pub enum InputItem {
    // ... existing variants ...
    Reasoning {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        encrypted_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        summary: Option<serde_json::Value>,
    },
}
```

```rust
// src/ai/openai_responses/types/response.rs
pub enum OutputItem {
    // ... existing variants ...
    Reasoning {
        id: String,
        #[serde(default)] status: Option<String>,
        #[serde(default)] summary: Option<serde_json::Value>,
        // ψ-α で追加:
        #[serde(default)]
        encrypted_content: Option<String>,
    },
}
```

注記: OpenAI 公式は reasoning item を output → input にそのまま積み直すことを許容している。DTO を output/input で分離しているのは VAC 側の都合（`#[serde(tag = "type")]` で enum 分岐するため）だが、wire format としては同形なのでシリアライズ結果は同一になるはず。`InputItem::Reasoning` のシリアライズ結果が output 時のそれと一致することを golden test で担保する。

### 4.2 Tool loop でのフロー

現行（χ-5）:

```
round 0: request.input = [system, user, ...history]
         response.output = [Reasoning?, FunctionCall, ...]
         → dispatch tool → tool_result を取得
round 1: request.input = [system, user, ...history, FunctionCall, FunctionCallOutput]
         response.output = [Reasoning?, Message(assistant)]
         → finalize
```

ψ-α 後（gpt-5 + passthrough 有効時のみ）:

```
round 0: request.input = [system, user, ...history]
         request.include = ["reasoning.encrypted_content"]
         response.output = [Reasoning(id=rs_A, encrypted=...), FunctionCall, ...]
         → dispatch tool
round 1: request.input = [system, user, ...history, Reasoning(rs_A), FunctionCall, FunctionCallOutput]
         request.include = ["reasoning.encrypted_content"]
         response.output = [Reasoning(id=rs_B, encrypted=...), Message(assistant)]
         → finalize
```

### 4.3 gpt-5 family detection

既存の [`src/ai/model_policy.rs`](../../src/ai/model_policy.rs) の `is_gpt5_family(&str)` を再利用する。新規追加は不要。

### 4.4 opt-out のユースケース

- **デバッグ時**: 既存 Chat Completions 時代の挙動を完全再現したい場合
- **実測時**: ψ-α-3 の実機計測で ON/OFF 比較する際に on-the-fly で切り替えられるように
- **モック**: unit test で encrypted pass-through が関与しないパスを検証するため

既定 `true`、`openai_reasoning_encrypted_passthrough = false` で opt-out。

---

## 5. Non-goals

- **`react()` 呼び出しを跨ぐ reasoning 持ち回り**: §1.3 で述べた通り、hot-reload / memory window trim でコヒーレンスが崩れるため対象外
- **`previous_response_id` / `store: true` 採用**: Phase ψ+（backlog）で別途検討
- **reasoning summaries の GUI 露出**: `summary` フィールドはデシリアライズだけするが、GUI 側での表示/保存は範囲外
- **reasoning token 単独の予算管理**: `max_output_tokens` / `openai_max_output_tokens` の既存制御に委ねる

---

## 6. Risks

| リスク | 対策 |
|---|---|
| OpenAI 側のフィールド命名変更 | blob 欠落は warn + fallback。既存動作に degrade するだけで落ちない |
| 非 gpt-5 モデルで誤って `include` を付ける | `is_gpt5_family` チェックを DTO 組み立て前に必ず行う。non-gpt5 ユーザは完全 no-op |
| `InputItem::Reasoning` のシリアライズ形が output と微妙にズレる | golden test で output → input round-trip を検証。ズレたら CI で即検出 |
| `include` フィールドが reasoning 以外の include 項目（将来の `file_search_call.results` 等）と衝突 | `Option<Vec<String>>` にしているので複数値を持てる。ψ-α では `["reasoning.encrypted_content"]` のみ指定 |

---

## 7. References

- OpenAI Responses API 公式: `/v1/responses` の request/response 仕様
- [`phase-chi-openai-responses.md`](phase-chi-openai-responses.md) §11.2: 本フェーズの設計メモ（本書で正式昇格）
- OpenAI reasoning guide: "Encrypted reasoning items" 節（[platform.openai.com/docs/guides/reasoning](https://platform.openai.com/docs/guides/reasoning)）
