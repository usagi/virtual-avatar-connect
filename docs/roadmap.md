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

---

## Active Phase

### Phase χ — OpenAI Responses API 全面移行

Chat Completions API から Responses API へ全面移行。streaming + tool loop + gpt-5 retry + overflow summary + hot-reload を **1 PR / 9 commit** で full_parity 移植。Option A（`reqwest` + 自前 DTO）で実装、shared crate 化を視野に境界設計する。

- [ ] χ-0 docs: roadmap.md / architecture.md 新設 + phase-chi-openai-responses.md 新設 + cross-link 更新
- [x] χ-1 feat(ai/responses): scaffold + Cargo deps 調整 + non-stream DTO types + `create` + golden tests
- [x] χ-2 feat(ai/responses): `StreamEvent` enum + SSE parser + モック stream tests
- [x] χ-3 refactor(ai/tools): Tool 定義 + `dispatch_tool_call` を Responses 型に移行
- [x] χ-4 refactor(ai/context,reload,model_policy): request assembly を `CreateResponseRequest` に置換
- [x] χ-5 refactor(ai/service,completion): streaming / tool-loop / gpt-5 retry を Responses 全面移行
- [ ] χ-6 refactor(conf): `openai_max_output_tokens` + レガシーフォールバック
- [ ] χ-7 docs: manual / conf.example / CHANGELOG 更新 + roadmap.md の χ tick
- [ ] χ-8 test: cargo test + svelte-check + 実機スモーク
  - [ ] χ-8.0 fix(flowgraph/docs): `node_catalog_md_up_to_date` テストを line-ending 正規化して CRLF 環境でも通るように（χ-5 以前からの既知 pre-existing bug、Windows の `core.autocrlf=true` で検出）
- 仕様書: [`roadmap/phase-chi-openai-responses.md`](roadmap/phase-chi-openai-responses.md)

---

## Backlog / Future

### Unscheduled Flowgraph Nodes

- [ ] `flowgraph.util.timer_interval`（周期タイマー、source ノード、details: [`roadmap/backlog-nodes.md`](roadmap/backlog-nodes.md) §1）

### Phase ψ+（TBD）

Phase χ Out-of-scope から派生する将来フェーズ候補:

- [ ] `previous_response_id` / `conversation` / `compact` API による server-side memory
- [ ] built-in tools（`web_search_preview` / `file_search` / `code_interpreter` / MCP tool）
- [ ] `vac-openai-responses` shared crate 化（un-discord-kaltsitpseudo との共有）
- [ ] ε-2 Tauri ネイティブウィンドウ化

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
