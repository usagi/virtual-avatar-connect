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

### Phase ψ-α — Encrypted Reasoning Passthrough

Phase χ の次の一手として、gpt-5 系の tool loop round 間で `include: ["reasoning.encrypted_content"]` を使い、`store: false` を維持したまま reasoning state を client 側で持ち回る **狭い範囲の最適化**。`drive_responses_tool_loop` の round 間で暗号化 reasoning blob を `output[]` → 次ラウンドの `input[]` に pass-through する。

- [x] ψ-α-0 docs: `phase-psi-alpha-encrypted-reasoning.md` 新設 + roadmap を backlog→Active に移行 + χ の §11.2 設計メモから phase doc に昇格
- [x] ψ-α-1 feat(ai/responses): DTO 拡張（`CreateResponseRequest.include` / `InputItem::Reasoning` / `OutputItem::Reasoning.encrypted_content`）+ golden tests 7 本追加
- [x] ψ-α-2 feat(ai/service,config): `openai_reasoning_encrypted_passthrough` (既定 `true`) + `drive_responses_tool_loop` の round 間 pass-through（gpt-5 系のみ有効、他モデルは no-op）+ resolver / helper unit tests 9 本
- [x] ψ-α-3 docs: CHANGELOG / conf-reference / conf.example 更新 + 実機スモーク（gpt-4o-mini で include 非付与・既存経路 bit-互換 / gpt-5-mini で include 付与・OpenAI 側 200 OK・SSE 正常完了）+ roadmap tick
- 仕様書: [`roadmap/phase-psi-alpha-encrypted-reasoning.md`](roadmap/phase-psi-alpha-encrypted-reasoning.md)
- scope: `drive_responses_tool_loop` の round-over-round に閉じる narrow change。`react()` 呼び出しを跨ぐ持ち回りはしない（hot-reload / memory window trim で context shape が変わるため）
- 実用的 reasoning token 節約の数値比較は VAC 同梱 tool がすべて 1 round で返るため将来 phase（user 作成 multi-round tools を使う）に委ねる

### Phase ν — GUI E2E testing with Playwright

Playwright による E2E テスト基盤を `gui/` 配下に閉じ込めて導入。初期 5 ケースのうち **3 ケース** (§3.1 / §3.4 / §3.5) を着地させ、残り 2 ケース (§3.2 canvas DnD / §3.3 editor 409 merge) は **ν-β** に分離する narrow-scoped な幕引きとした。`webServer` が `cargo run --release -- conf.fixture.e2e.toml` を起動し、外部 IO ゼロの fixture で auth / HTTP / WS / widget 操作 / flowgraph trigger までを 3 specs / 8.4s で回帰検知できる状態になった。

- [x] ν-0 docs: `phase-nu-gui-e2e-playwright.md` 本文書き下ろし + 5 ケース仕様 + fixture 設計 + webServer 戦略 + セレクタ規約確定
- [x] ν-1 chore(gui): `@playwright/test` 追加 + `playwright.config.ts` + `gui/tests/e2e/` + `conf.fixture.e2e.toml`（外部 IO ゼロ）+ fixture flowgraph 同梱
- [x] ν-2 test(gui): 3 ケース実装完了（`control-panel-smoke` / `channels-ws-live-update` / `live-quick-add-learn-undo`）— §3.2 / §3.3 は ν-β へ分離
- [x] ν-3 docs: CHANGELOG / manual 追記（run 手順 + CI optional 方針）+ roadmap tick（commit `4f8b15d`）
- 仕様書: [`roadmap/phase-nu-gui-e2e-playwright.md`](roadmap/phase-nu-gui-e2e-playwright.md)
- scope: narrow-scoped。`gui/` 配下に閉じ、Rust 側 `Cargo.toml` や CI には一切触れない。visual regression / 多ブラウザ matrix / component testing は ν+ に送る
- Linux / Windows どちらでも手元で回せることが必須。CI 化は optional（ν-β 以降で検討）

---

### Phase ν-β — Flowgraph Canvas + Dictionary Editor E2E

ν-2 から分離した後続フェーズ。Dictionary Editor の 409 race condition、Flowgraph Canvas の編集 → Ctrl+S 往復、engine 側の Table default coerce 失敗 (`MissingRequiredInput`) の 3 点を一気に着地させた。最終的に `gui/tests/e2e/` は **5 specs / 1 worker / ~9 s** で全通し、fixture flowgraph は `table.from_json` 補助ノードを外して `in/log/learn/forget` の 4 ノード最小構成に戻せた（§6.4 の追補条件が全部成立）。

- [x] ν-β-3 fix(flowgraph/table): `Table::from_json_array(&[], None) → Table::empty()` で空 Table default の coerce 経路を通す + `PortSpec::with_default` の round-trip test + fixture から `dict_src` 削除
- [x] ν-β-1 test(gui): §3.3 `dictionary-editor-409-merge.spec.ts`（PATCH 409 → `DictionaryConflictDialog` 3-way merge → 自分の編集を強制 → cleanup）
- [x] ν-β-2 test(gui): §3.2 `flowgraph-canvas-basic.spec.ts`（Palette click-add → dirty badge → Ctrl+S → PUT 200 → clean、edge drag / beforeunload は ν-β+ に送る）
- 仕様書: [`roadmap/phase-nu-gui-e2e-playwright.md`](roadmap/phase-nu-gui-e2e-playwright.md) §3.2〜§3.3 および §6.4

---

## Active Phases

（現在 active な phase はありません。次候補は "Flowgraph 機能向上" / "ε-2 Tauri ネイティブウィンドウ化"、詳細は下記 Backlog / Future を参照）

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

### Flowgraph 機能向上（TBD）

ψ-α / ν 完了後の次フェーズ候補（ユーザー意向）。ν-β の canvas DnD E2E が足場になるので、小規模 UX 改善（node catalog 絞り込み / 配線補助）から engine 拡張（Table default coerce fix、新 node 追加）まで、phase doc を新設して具体サブフェーズを確定する予定。ν-β で発見済みの engine 修正 (`Table::empty()` default coerce) も取り込む。

### ε-2 Tauri ネイティブウィンドウ化

ψ-α / ν / ν-β / Flowgraph 機能拡張の次に着手検討（ユーザー意向として "GUI の Tauri 化" を積んでいる）。仕様書: [`roadmap/phase-epsilon-shutdown-and-tauri.md`](roadmap/phase-epsilon-shutdown-and-tauri.md)

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
