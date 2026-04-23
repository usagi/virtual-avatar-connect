# VAC Implementation Roadmap

Virtual Avatar Connect の全フェーズ × サブフェーズ単位のチェックボックス一覧。
各フェーズの詳細設計は `docs/roadmap/phase-*.md`（UNVET の `docs/plan.md` 相当）を参照。
全体方針・レイヤ構成・Commit Granularity Rule は [`architecture.md`](architecture.md) を参照。

---

## Completed Phases

### Phase δ — Flowgraph Baseline

- [x] δ-0 仕様確定（型付き IO コネクター + exec pin + DAG）
- [x] δ-1〜δ-8 コアノード群実装
- [x] δ-9 Part A-C: Flowgraph Runtime 並走稼働（v0.9.0）
- [x] δ-X `flowgraph.command.set` ノード（V1 scene switcher Flowgraph ネイティブ化）
- 仕様書: [`roadmap/phase-delta-spec.md`](roadmap/phase-delta-spec.md)

### Phase ε — Shutdown 統合と Tauri 移行段取り

- [x] ε-1 ShutdownBroker（Ctrl+C / POST /shutdown / Fatal / Tauri の 4 経路集約）
- [ ] ε-2 Tauri ネイティブウィンドウ化 / CLI 可視性ポリシー（**保留**、GUI 安定後に再開）
- 仕様書: [`roadmap/phase-epsilon-shutdown-and-tauri.md`](roadmap/phase-epsilon-shutdown-and-tauri.md)

### Phase ζ — v2 → main merge 準備

- [x] ζ-3 Flowgraph reload 時のブリッジ再配線
- [x] ζ-4 AI-Twitch ハイブリッド方針の conf コメント化
- 関連: [`roadmap/v2-merge-pr.md`](roadmap/v2-merge-pr.md)

### Phase η — Dictionary / Table Unification

- [x] η-0 docs: 11 カラム辞書仕様書固定
- [x] η-1a feat(flowgraph): `SocketType::Table` 追加
- [x] η-1b feat(flowgraph): `src/flowgraph/table.rs` 新設
- [x] η-2 feat(flowgraph/nodes/table): `flowgraph.table.*` 4 ノード
- [x] η-3 feat(flowgraph/nodes/dictionary): `flowgraph.dictionary.*` 4 ノード刷新
- [x] η-4 refactor: `flowgraph.dictionary.command` 削除 + commands.tsv 方式
- [x] η-5 feat(bin): `virtual-avatar-connect-migrate-dict` V1→η migrate CLI
- [x] η-6 feat(gui): Table ポート視覚区別
- 仕様書: [`roadmap/phase-eta-dictionary-unification.md`](roadmap/phase-eta-dictionary-unification.md)

### Phase φ — Control API + Dictionary Editor + Live Quick-Add

- [x] φ-0 docs: Control API / Table CRUD / Trigger 仕様書
- [x] φ-1 feat(web_interface/control): Table CRUD API 実装
- [x] φ-2 feat(web_interface/control): Flowgraph Trigger API 実装
- [x] φ-3a feat(gui): `types.ts` / `api.ts` に Control Table / Trigger 型追加
- [x] φ-3b feat(gui): Dictionary Table / DictionaryEditorPane 骨格
- [x] φ-3c feat(gui): DictionaryEntryForm（11 カラム編集モーダル）
- [x] φ-3d feat(gui): DictionaryConflictDialog（3-way merge）
- [x] φ-3e feat(gui): LiveTab 統合
- [x] φ-4 feat(gui): Live Quick-Add Widget + forget_node_id
- [x] φ-5 docs: `[[control_api.tables]]` / quick_add reference + tutorial
- [x] φ-6 feat(flowgraph,gui): Flowgraph Editor 上の Trigger ボタン
- 仕様書: [`roadmap/phase-phi-control-api-dictionary-editor.md`](roadmap/phase-phi-control-api-dictionary-editor.md)

### Phase χ — OpenAI Responses API 全面移行

Chat Completions API から Responses API へ全面移行。streaming + tool loop + gpt-5 retry + overflow summary + hot-reload を **1 PR / 9 commit** で full_parity 移植。Option A（`reqwest` + 自前 DTO）で実装、shared crate 化を視野に境界設計する。

- [x] χ-0 docs: roadmap.md / architecture.md 新設 + phase-chi-openai-responses.md 新設 + cross-link 更新
- [x] χ-1 feat(ai/responses): scaffold + Cargo deps 調整 + non-stream DTO types + `create` + golden tests
- [x] χ-2 feat(ai/responses): `StreamEvent` enum + SSE parser + モック stream tests
- [x] χ-3 refactor(ai/tools): Tool 定義 + `dispatch_tool_call` を Responses 型に移行
- [x] χ-4 refactor(ai/context,reload,model_policy): request assembly を `CreateResponseRequest` に置換
- [x] χ-5 refactor(ai/service,completion): streaming / tool-loop / gpt-5 retry を Responses 全面移行
- [x] χ-6 refactor(conf): `openai_max_output_tokens` + レガシーフォールバック
- [x] χ-7 docs: manual / conf.example / CHANGELOG 更新 + roadmap.md の χ tick
- [x] χ-8 test: cargo test + svelte-check + 実機スモーク + 観測器追加
  - [x] χ-8.0 fix(flowgraph/docs): `node_catalog_md_up_to_date` テストを line-ending 正規化して CRLF 環境でも通るように（χ-5 以前からの既知 pre-existing bug、Windows の `core.autocrlf=true` で検出）
  - [x] χ-8.1 test(ai/responses): `cargo test --lib --release` 481 pass / 0 fail / 1 ignored、`gui/ npm run check` / `npm run build` 緑、実機スモーク (gpt-4o-mini happy / gpt-4o-mini + tools / gpt-5-mini + `reasoning.effort=low`) 全成功
  - [x] χ-8.2 feat(ai/responses): `ResponsesClient` / `drive_responses_tool_loop` に送信・status・SSE イベント受信の debug/trace ログを追加。`connect_timeout(10s)` を明示し、Windows TLS 交渉ストールで total timeout が事実上無効化されるケースの耐性を上げる
- 仕様書: [`roadmap/phase-chi-openai-responses.md`](roadmap/phase-chi-openai-responses.md)

---

## Active Phases

### Phase ψ-α — Encrypted Reasoning Passthrough

Phase χ 完了後の次の一手。gpt-5 系の tool loop round 間で `include: ["reasoning.encrypted_content"]` を使い、`store: false` を維持したまま reasoning state を client 側で持ち回る **狭い範囲の最適化**。`drive_responses_tool_loop` の round 間で暗号化 reasoning blob を `output[]` → 次ラウンドの `input[]` に pass-through する。

- [ ] ψ-α-0 docs: `phase-psi-alpha-encrypted-reasoning.md` 新設 + roadmap を backlog→Active に移行 + χ の §11.2 設計メモから phase doc に昇格
- [ ] ψ-α-1 feat(ai/responses): DTO 拡張（`CreateResponseRequest.include` / `InputItem::Reasoning` / `OutputItem::Reasoning.encrypted_content`）+ golden tests
- [ ] ψ-α-2 feat(ai/service,config): `openai_reasoning_encrypted_passthrough` (既定 `true`) + `drive_responses_tool_loop` の round 間 pass-through（gpt-5 系のみ有効、他モデルは no-op）
- [ ] ψ-α-3 docs: CHANGELOG / conf-reference / conf.example 更新 + 実機計測（reasoning tokens / latency / $）を記録 + roadmap tick
- 仕様書: [`roadmap/phase-psi-alpha-encrypted-reasoning.md`](roadmap/phase-psi-alpha-encrypted-reasoning.md)
- scope: `drive_responses_tool_loop` の round-over-round に閉じる narrow change。`react()` 呼び出しを跨ぐ持ち回りはしない（hot-reload / memory window trim で context shape が変わるため）

### Phase ν — GUI E2E testing with Playwright

現状 `gui/` に自動テストは 0 件（`svelte-check` の型検査のみ）。ε / φ 系で GUI 機能面積が拡大しており、手動スモークでの回帰検知が限界に近づきつつあるため、Playwright による E2E テスト基盤を導入する独立フェーズ。ψ-α 完了後に着手予定（ψ-α と独立だが、ランタイム変更が連続する方がメモリコストが低いため）。

動機（E2E でしか拾えない挙動が溜まっている）:

- Phase φ の **Dictionary Editor Pane**（optimistic lock / 409 → 3-way merge ダイアログ / Live Quick-Add の Learn + Undo）
- **Flowgraph Canvas**（γ-4a / γ-4a.0 のノード追加・接続・削除・Save-dirty 表示・Ctrl+S・beforeunload ガード）
- **Live Tab** / **Managed App Drawer**（γ-2 の restart + status polling）
- OBS Browser Source（`/browser-output/*`）の WebKit 挙動検証

- [ ] ν-0 docs: `phase-nu-gui-e2e-playwright.md` 新設 + スコープ / fixture 設計 / 5 初期ケース仕様の確定
- [ ] ν-1 chore(gui): `playwright.config.ts` + `gui/tests/e2e/` 配置、`webServer` で VAC を `conf.fixture.e2e.toml`（外部依存ゼロ）で起動する仕組み
- [ ] ν-2 test(gui): 初期 5 ケース `control-panel-smoke` / `flowgraph-canvas-basic` / `dictionary-editor-409-merge` / `live-quick-add-learn-undo` / `channels-ws-live-update`
- [ ] ν-3 docs: CHANGELOG / manual 追記（run 手順 + CI optional 方針）+ roadmap tick
- 仕様書: [`roadmap/phase-nu-gui-e2e-playwright.md`](roadmap/phase-nu-gui-e2e-playwright.md)
- Linux / Windows どちらでも手元で回せることが必須。CI 化は optional（ν-2 以降）

---

## Backlog / Future

### Unscheduled Flowgraph Nodes

- [ ] `flowgraph.util.timer_interval`（周期タイマー、source ノード、details: [`roadmap/backlog-nodes.md`](roadmap/backlog-nodes.md) §1）

### Phase ψ+（TBD）

Phase χ / ψ-α を経てなお残る将来フェーズ候補:

- [ ] `previous_response_id` / `conversation` / `compact` API による server-side memory（ψ-α を経てなお解決しない長期会話ユースケースが残る場合のみ検討。VAC の `include_all` / `overflow_summary` / hot-reload と構造的に衝突するため、既定は `store: false` を維持。設計判断メモ: [`roadmap/phase-chi-openai-responses.md`](roadmap/phase-chi-openai-responses.md) §11.1）
- [ ] built-in tools（`web_search_preview` / `file_search` / `code_interpreter` / MCP tool）
- [ ] `vac-openai-responses` shared crate 化（un-discord-kaltsitpseudo との共有）

### Phase ν+（TBD）: E2E 拡張

- [ ] φ / γ 後続サーフェス（Managed App Drawer 詳細、BOS Preview の縦画面トグル 等）
- [ ] visual regression（`toHaveScreenshot`）
- [ ] GitHub Actions 上での chromium / firefox / webkit マトリクス

### ε-2 Tauri ネイティブウィンドウ化

ψ-α / ν 完了 + Flowgraph 機能拡張の次点として着手検討。仕様書: [`roadmap/phase-epsilon-shutdown-and-tauri.md`](roadmap/phase-epsilon-shutdown-and-tauri.md)

---

## Commit Unit Convention

- サブフェーズ ID を commit message の先頭に付ける: `χ-0 docs: ...` / `χ-1 feat(ai/responses): ...`
- Conventional Commits タイプ（`feat` / `fix` / `refactor` / `docs` / `test` / `chore`）をサブフェーズ ID の後ろに続ける
- scope はモジュール経路で指定: `feat(ai/responses)` / `refactor(web_interface/control)` 等
- 1 commit = 1 behavior topic。横断的な変更は複数 commit に分割する
- Breaking change は必ず `CHANGELOG.md` の `### Breaking changes (<phase>)` に明記する

---

## Related Documents

- [`architecture.md`](architecture.md) — VAC のレイヤ構成と Commit Granularity Rule
- [`roadmap/phase-delta-spec.md`](roadmap/phase-delta-spec.md) — Flowgraph 基幹仕様
- [`manual/index.md`](manual/index.md) — ユーザー向けマニュアル
- [`../CHANGELOG.md`](../CHANGELOG.md) — Phase 単位の変更履歴
