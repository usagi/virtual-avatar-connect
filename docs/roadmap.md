# VAC Implementation Roadmap

Virtual Avatar Connect の全フェーズ × サブフェーズ単位のチェックボックス一覧。
各フェーズの詳細設計は `docs/roadmap/phase-*.md`（UNVET の `docs/plan.md` 相当）を参照。
全体方針・レイヤ構成・Commit Granularity Rule は [`architecture.md`](architecture.md) を参照。
VAC Flowgraph を常駐型汎用データフロー処理エンジン、および汎用プログラミング言語に近い実行記述へ伸ばす横断計画は [`roadmap/flowgraph-language-roadmap.md`](roadmap/flowgraph-language-roadmap.md) を参照。
VAC 常駐化に伴う配信・日常・仕事・睡眠などの動作状態切替計画は [`roadmap/runtime-mode-roadmap.md`](roadmap/runtime-mode-roadmap.md) を参照。
v2 GUI を常駐ランタイムの管制卓と Flowgraph Studio へ再設計する計画は [`roadmap/gui-redesign-roadmap.md`](roadmap/gui-redesign-roadmap.md) を参照。

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
- [ ] ε-2 Tauri ネイティブウィンドウ化 / CLI 可視性ポリシー（**保留**、GUI 安定後に再開）。**着手順の方針**: crate 再構造化 → `virtual-avatar-connect-cli` / `virtual-avatar-connect-desktop`（仮称）の 2 runner → **Tauri は desktop にのみ**組み込む（[`architecture.md`](architecture.md)「実行入口」、[`roadmap/v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md) §1.1）。
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

### Phase ο — Flowgraph Enhancement I (計算系 + 時間 + signal util + GUI 小改善)

Flowgraph 機能向上の第 1 波。旧 ο-0 の本数想定 **78 (42+1+20+5+5+5)** では §3.4 が **timer 1 + time 案 4 = 5** 本だった。time 4 種（旧 `flowgraph.time.*` 想定）は **Phase π** の `flowgraph.datetime.*` **8 ノード**に置換し、**ο-4** は **`flowgraph.util.timer_interval`（Stateful）** のみにスコープ縮小。合算 **(42+1+20+1+5+5+5) + 8 = 82 本**（**Phase ο 側 engine+GUI 74** + **π 8**）。外部 IO は **ρ → σ → τ → υ → ω** の backlog 順（旧 π〜υ 1 文字繰り下げ後の命名）で追って取る。

- [x] ο-0 docs: `phase-omicron-flowgraph-enhancement.md` 新設 + roadmap.md の Active 差し替え + backlog-nodes.md §1 を ο-4 昇格 pointer 化（+ 追補: angle normalization 4 / 双曲線 6 ノード追加 + Phase ξ 依存明記）
- [x] ο-1 feat(flowgraph/math): §3.1 **42 ノード**追加（abs/min/max/clamp/lerp/inverse_lerp/remap/smoothstep/trig/arctrig/atan2/hyperbolic/arc-hyperbolic/sqrt/pow/exp/log/sign/floor/ceil/round/deg↔rad/normalize_angle × 4）。全 Quantity-aware、30 unit tests 全緑、node-catalog.md 再生成済み。ο-0 docs の「37」表記は実装時に 42 へ正確化
- [x] ο-2 feat(flowgraph/easing): §3.2 `flowgraph.easing.apply` + curve enum 19 種 (linear + quad/cubic/sine/expo/elastic/bounce × in/out/inOut) + `PropertySpec.choices` 追加 + GUI `FlowgraphPropertyEditor` の `<select>` dropdown hook。13 unit tests 全緑、node-catalog.md 再生成済み
- [x] ο-3 feat(flowgraph/vec): §3.3 vec2 / vec3 × 10 ノード ✕ 2 = **20 ノード**（make/unpack/add/sub/scale/dot/length/normalize/lerp/distance、JSON 配列表現）。`decode_vec::<N>` / `encode_vec::<N>` の `const N: usize` generic helper + 6 種マクロで型安全に実装、19 unit tests 全緑（round-trip / 零ベクトル normalize / 次元不一致 error / 非有限値 reject 含む）、node-catalog.md 再生成済み
- [x] ο-4 feat(flowgraph/util): §3.4 `flowgraph.util.timer_interval`（[backlog-nodes.md §1](roadmap/backlog-nodes.md)）。Stateful + `ctx.trigger`、lazy 初回の `sources` engine 例外、lib test + `node-catalog`。**旧案 `flowgraph.time.*` 4 種は Phase π の `flowgraph.datetime.*` 8 ノードに置換済み**（[phase-pi-datetime-system.md](roadmap/phase-pi-datetime-system.md) §4.9 / [datetime-system.md](manual/datetime-system.md)）
- [x] ο-5 feat(flowgraph/util,random,noise): §3.5 signal util 5 種 + §3.6 random 3 + Perlin 1D/2D（`noise = 0.9`）、`node-catalog` 再生成、lib test +10、`edge_detect` 実時間 integration（`timer_interval` + `state.bool` + `log`）
- [x] ο-6 feat(gui): §3.7 palette カテゴリ非表示トグル（`localStorage`）+ pane DnD drop（`FlowgraphPaneDropBridge` + `addCatalogNodeAt`）+ `Ctrl/Cmd+D` 複製 + `flowgraph-canvas-basic.spec.ts` 回帰 3 本（E2E 前に `gui` を `npm run build`）
- [x] ο-7 docs: CHANGELOG に ο-0..ο-7 総括節、`BLESS_NODE_CATALOG=1 cargo test --lib node_catalog_md_up_to_date` で `docs/manual/node-catalog.md` 整合確認、本ファイルで Phase ο を Completed へ移動、`npm run check` / `cargo test --lib` / `npx playwright test` 緑確認
- 仕様書: [`roadmap/phase-omicron-flowgraph-enhancement.md`](roadmap/phase-omicron-flowgraph-enhancement.md)
- scope: narrow-scoped。engine 内完結 Pure ノード + GUI 小改善に閉じる。新規 crate 依存は `noise` 1 件のみ。Breaking change なし

### Phase π — DateTime Type System (jiff 採用 + chrono 全面置換)

Flowgraph engine に **絶対時刻を表す `DateTime` 型**を第一級概念として導入した基盤フェーズ（**完了**）。Phase ξ の単位次元と同じ「型は事故を防ぐ砦」哲学。`chrono` 直接依存を撤去し **`jiff`** に全面置換。`Duration` は ξ の `Quantity<time>` に統一。設計上の先決条件（ο-4 `timer_interval` など）を満たしたうえで実装された。

- [x] π-0 docs: `phase-pi-datetime-system.md` 新設 + roadmap.md の phase 名・順序の再編（旧 π/ρ/σ/τ/υ を 1 文字繰り下げて ρ/σ/τ/υ/ω に、DateTime を新 π に割り当て）+ cross-reference 修正
- [x] π-1 feat(deps): `jiff = "0.2.24"` (features: `serde` + デフォルト `tz-system` / `tzdb-*`) 追加 + `tests/jiff_smoke.rs` 14 tests 全緑（Timestamp / SignedDuration / Offset / Zoned / serde round-trip）。副産物: `Offset` は `FromStr` 非実装、`DateTimeParser::parse_time_zone` は bare `Z` を拒否する仕様を pin 止め（phase doc §3.5 更新）
- [x] π-2 refactor(chrono->jiff): 既存 chrono 使用 21 箇所を jiff に全面置換（3 commit 構成: batch1 leaf 14 / batch2 dictionary TTL 境界 + 7 境界テスト / batch3 struct field 5 + 4 serde round-trip snapshot、計 702→706 lib tests all green）。wire format drift `+00:00` → `Z` は RFC3339 互換範囲として許容し commit message に明記
- [x] π-3 chore(deps): `Cargo.toml` から `chrono` 直接依存を解除、`src/**` は `use chrono` ゼロ、`cargo tree -i chrono` で直接依存なしを確認（間接依存は `twitch-irc v6.0.0` 経由で残存、これは twitch-irc 側の内部実装で π スコープ外）、全 706 lib tests green
- [x] π-4 feat(flowgraph/datetime): `SocketType::DateTime` + `SocketValue::DateTime` + engine 側 String ↔ DateTime 暗黙 coerce + `FlowgraphInstanceConfig.default_timezone: Option<String>`（未設定時 UTC、FixedOffset `+09:00` 形式のみ、IANA tz 非対応）+ naive datetime パース policy（config default tz 適用）
- [x] π-5 feat(flowgraph/nodes/datetime): 8 ノード（`now` / `parse` / `format` / `add_duration` / `sub_duration` / `diff` / `epoch_ms` / `from_epoch_ms`）+ `get_required_datetime` + lib test +30、`node-catalog` 再生成済
- [x] π-6 docs: CHANGELOG + `docs/manual/datetime-system.md` 新設 + `docs/manual/index.md` + [phase-omicron-flowgraph-enhancement.md](roadmap/phase-omicron-flowgraph-enhancement.md) §3.4/§5.4/§6.4 更新 + roadmap 本節 tick（`node-catalog` は π-5 時点で更新済）
- 仕様書: [`roadmap/phase-pi-datetime-system.md`](roadmap/phase-pi-datetime-system.md)
- scope: chrono → jiff 全面移行 + Flowgraph `DateTime` 型。Breaking change なし（wire 互換・config 既定で後方互換）。IANA tz / DST / `Span`（暦幅）/ 独自 affine 単位は **π+** 扱い
- 順序: **π-0..π-6 完了**。`flowgraph.datetime.*` による日時ワイヤは本フェーズで揃い、**ρ → σ → τ → υ → ω** backlog とは独立

### Phase ξ — Dimensional Quantity System (SI 準拠の単位次元システム)（完了）

Flowgraph engine に **SI 準拠の単位次元システム**を第一級概念として導入した基盤フェーズ。数値に unit を付与、unit は次元（L·M·T·I·Θ·N·J + 疑似次元 Angle の 8 成分）を持つ。strict default + 明示 `flowgraph.unit.strip`。IO 系は pass-through。既存 flow は dimensionless fallback で完全後方互換。

- [x] ξ-0 docs: `phase-ksi-dimensional-quantity-system.md` 新設 + roadmap.md への Phase ξ 追加 + Phase ο doc の依存注記
- [x] ξ-1 feat(flowgraph/quantity): `Dimension` / `Unit` / `Quantity` 型 + SI 基本 7 単位 + 主要誘導単位 + SI 接頭辞 20 種 + Angle 疑似次元（rad / deg）+ 温度 delta 分離 (K / ΔK) + unit 文字列 parser + unit test 30+
- [x] ξ-2 feat(flowgraph/nodes/unit): `flowgraph.unit.*` 操作ノード 7 種 + `SocketType::Quantity` / `SocketValue::Quantity` + TOML / JSON wire format + `Unit::to_si_base` atom-canonical 係数込み
- [x] ξ-3 refactor(flowgraph): `Float ↔ Quantity` 暗黙 coerce + `flowgraph.math.float_*` の Quantity 演算化
- [x] ξ-4 feat(flowgraph/util): `Quantity → String` 暗黙 coerce + `flowgraph.util.format` + log / channel.emit の unit-aware 化
- [x] ξ-5 feat(gui): `FlowgraphNodeCard.svelte` で Quantity ポートの handle 色分け（次元 family）+ default から復元できる場合のみ unit バッジ + tooltip。`FlowgraphCanvas` で engine 互換の型接続（Float↔Quantity 等）と型不一致エッジの赤破線表示。`FlowgraphPropertyEditor` で `unit` / `target_unit` / `unit_override` の `GET /flowgraph/parse-unit` 検証。node-catalog JSON に `quantity_dim` 等を注入。E2E `flowgraph-canvas-basic` に Quantity handle 回帰を追加。
- [x] ξ-6 docs: `docs/manual/dimensional-quantity-system.md` ほか
- 仕様書: [`roadmap/phase-ksi-dimensional-quantity-system.md`](roadmap/phase-ksi-dimensional-quantity-system.md)
- 順序: **ξ-0..ξ-6 完了**。次の本編候補は Backlog（[Phase ρ](#phase-ρ--osc--vmc--vrc-bridge-tbd) など）。**閉集合 string・ライブラリ再利用 v0** は [Phase λ](roadmap/phase-lambda-flowgraph-enum-and-library.md)（本ファイル Completed 節）で完了。

### Phase λ — Flowgraph: Enum 型システム + ライブラリ再利用（v0 完了）

閉集合 string（`PortSpec` メタ）、ユーザ定義 `[[enums]]`、ライブラリ境界ノード（`flowgraph.library.*`）、`[meta]` 拡張（author/name/version / `library_uses`）、依存グラフの閉路検出を一つの設計線で実装した。設計の単一ソースは [`roadmap/phase-lambda-flowgraph-enum-and-library.md`](roadmap/phase-lambda-flowgraph-enum-and-library.md)。δ 当初の `FlowgraphFile` 3 セクション方針は **後方互換のもとで拡張**（同 phase doc §4、[`phase-delta-spec.md`](roadmap/phase-delta-spec.md) §8.6）。

- [x] λ-0 docs: `phase-lambda-flowgraph-enum-and-library.md` 新設 + `roadmap.md` Active 化 + `phase-delta-spec.md` / `architecture.md` への cross-link
- [x] λ-1 feat(flowgraph): Enum MVP（`closed_string_variants` + engine / loader / GUI + `tts.speak` の `engine`）
- [x] λ-2 feat(flowgraph,gui): `[[enums]]` パース・診断・パレット `user_defined`
- [x] λ-3 feat(flowgraph): `flowgraph.library.input` / `flowgraph.library.output` スタブ登録
- [x] λ-4 feat(flowgraph): `FileMeta` 拡張 + `library_uses` + 閉路検出
- [x] λ-5 docs: CHANGELOG + manual 断片 + example 更新
- ユーザ向け: [`manual/flowgraph-enum-and-library.md`](manual/flowgraph-enum-and-library.md) / 例: [`flowgraph.example/lambda-demo/main.flowgraph.toml`](../flowgraph.example/lambda-demo/main.flowgraph.toml)
- **λ+（未スケジュール）**: 境界ポートの接続駆動増減、Exec 境界、fragment との二重経路整理などは phase λ 文書に従い別マイルストーンで扱う。

---

## Active Phases

### Phase M — VMC パススルーと motion 層（[`v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md)）

ギリシャ文字フェーズ（δ / φ …）とは別ラベルの **M0〜M5** で追う。全体方針・アーキ図は [`roadmap/v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md)。**M0 / M1 の実装メモ**は [`roadmap/phase-mu-vmc-motion-m0.md`](roadmap/phase-mu-vmc-motion-m0.md)（M1 は同文書 §8）。

- [x] M-0 docs: `phase-mu-vmc-motion-m0.md` 新設 + `roadmap.md` Active 化 + `architecture.md` / `conf-reference.md` + `v2-vmc-and-restructure.md` cross-link + `conf.example-motion.toml`
- [x] M-0 feat(motion): `src/motion/`（`vmc_raw` + `router` + `osc` プレースホルダ）+ `[motion]` / `MotionHandles` 起動・`ShutdownBroker` 連携
- [x] M-1 feat(flowgraph,bridges): `flowgraph.ingress.vmc_udp` + `vmc_ingress` ブリッジ + `` `TriggerEvent` `` 投入（`phase-mu` §8、`flowgraph.example/vmc-udp-ingress`）
- [x] M-2 feat(motion): パススルー・ハブ運用の conf / ログ整備（複数受信ソケットの運用例）
- [ ] M-3 feat(web_interface,gui): Control API `POST /api/v1/vmc/*` + トレイ/Web UI の転送先管理
- [x] M-4a feat(flowgraph,motion,deps): `flowgraph.motion.vmc_parse`（Base64→OSC→JSON）+ Phase ρ 先取り `flowgraph.osc.send`（UDP）+ `rosc` + `flowgraph::fixture_runner` 最小 + `src/app_core.rs`（Step 7 一段）
- [x] M-4 feat(flowgraph): `MotionFrame` 第一級型 + `motion_frame` ソケット（`json` と coerce 往復）+ `flowgraph.motion.vmc_parse` / `filter` / `map`（ワイヤ表現は `byte_len` + `osc_messages`。**head_pose 等の意味 IR**は M5 以降 / `v2-vmc` §M4 参照）
- [ ] M-5 docs+flowgraph: 表情・ジェスチャ等の用途拡張（`v2-vmc` §5 参照）

### v2 crate / runner / GUI 同梱（再構造化メタ）

ギリシャ文字フェーズに先行して **ワークスペース化〜配布形態**を追うチェックリスト。正本: [`roadmap/v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md) §1.1 / §1.2 / §3（`vac-gui-assets` 図・移行手順 Step 4 以降）。実装順と担当境界は [`roadmap/crate-runner-desktop-restructure.md`](roadmap/crate-runner-desktop-restructure.md)。

- [x] Step 4（一段）: [`architecture.md`](architecture.md) レイヤ境界表 + `motion` / `bridges` モジュール契約。`state→web_interface` 型依存は `twitch_oauth_sessions` / `control_events` で中立化済み
- [x] Step 5（部分）: ワークスペース化（`vac-gui-assets` のみメンバ追加、`default-members = ["."]`）。他 crate の分割は継続
- [x] Step 6a: Cargo feature **`embed-gui`** — `/gui/*` メモリ配信（`gui_embedded` / `gui_path`）
- [x] Step 6b（crate）: **`vac-gui-assets`** メンバ（`include_dir` + `build.rs`）。**CI は §1.3 どおり未着手**
- [x] Step 7（一段）: `src/app_core.rs` の `run_vac_application`（`ShutdownBroker` 以降〜 cleanup）+ `run_services` 集約。将来 `vac-app` への切り出し境界
- [x] Step 7d: crate / runner / desktop 再編の実装順と担当境界を固定（[`roadmap/crate-runner-desktop-restructure.md`](roadmap/crate-runner-desktop-restructure.md)）
- [ ] Step 8: CLI / desktop の 2 runner（仮称どおり）
- [ ] Step 9: desktop に Tauri + 同梱静的 + トレイ

---

## Backlog / Future

### Flowgraph Language Roadmap

VAC Flowgraph を「設定ファイル」ではなく、常駐型ランタイムへロードされる **プログラム**として扱うための横断計画。既存 Phase δ / λ / ξ / π / ο / M / ρ... を置き換えず、サブグラフ関数化（λ+）、コレクション処理、第一級 `Result` / `Record` / `Bytes` / `MotionFrame` 型、永続 state、capability、debug/test runner を順序立てる上位設計として管理する。

- 仕様草案: [`roadmap/flowgraph-language-roadmap.md`](roadmap/flowgraph-language-roadmap.md)
- 次の文書化候補: Flowgraph Language Spec、λ+ graph-as-node detailed spec、Flowgraph fixture test runner 設計

### Runtime Mode Roadmap

VAC を常駐型データフローアプリとして動かし続ける前提で、配信・日常・仕事・睡眠・RTA などのユーザー状態に合わせた runtime mode 切替を扱う横断計画。`conf` profile の丸ごと切替ではなく、Flowgraph file / group activation、AI assistant、通知、Managed App desired state を runtime 差分適用する設計として管理する。

- 仕様草案: [`roadmap/runtime-mode-roadmap.md`](roadmap/runtime-mode-roadmap.md)
- 初期 Flowgraph node 方針: `flowgraph.mode.get` / `flowgraph.mode.equals` / `flowgraph.mode.transit` の 3 種に絞る。`on_transit` 専用 ingress node は初期実装しない。

### GUI Redesign Roadmap

VAC GUI を「機能別の設定パネル」から「常駐ランタイムの管制卓 + Flowgraph Studio」へ再設計する横断計画。トップレベル IA を `Now / Modes / Flowgraph Studio / Resources / Observability / Settings` へ変更し、現在状態の把握、Runtime Mode、Flowgraph 編集、外部リソース、観測、設定を分離して進める。

- 仕様草案: [`roadmap/gui-redesign-roadmap.md`](roadmap/gui-redesign-roadmap.md)
- 初期実装スコープ: `Now` first screen、`Modes` placeholder、新 IA への hash fallback、既存 Flowgraph editor 内部は温存

### Phase ρ — OSC / VMC / VRC bridge (TBD)

**Phase M0〜M2** で VMC **生 UDP** のパススルーと（将来）Flowgraph ingress を先に切る。[`v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md) / [`phase-mu-vmc-motion-m0.md`](roadmap/phase-mu-vmc-motion-m0.md)。本 Phase ρ は **OSC の型付き送受信・VRChat ヘルパー・Flowgraph ノード群**（`rosc` 等）の横串として従来どおり backlog に残す（M 系完了後の接続を想定）。

Flowgraph から OSC（Open Sound Control）を使ってアバターアプリ・VRChat・その他 OSC 対応ソフト（VTube Studio の一部 / LiveLinkFace 等）を制御する基盤。VAC を「独自 avatar renderer を持つ前に、既存アバターアプリを Flowgraph から総合制御するハブ」に格上げする phase。

- [x] `rosc` crate 追加（`Cargo.toml`）— デコードは `src/motion/vmc_osc.rs`、単発送信は `flowgraph.osc.send`
- [x] `src/flowgraph/osc.rs` 共有基盤（JSON args → OSC エンコード・UDP 単発送出）。受信 ingress 型は従来どおり `bridges` / `motion` 側
- [x] `flowgraph.osc.send`（host / port / path / args JSON、`on_success` / `on_error`）
- [x] `flowgraph.ingress.osc_udp`（`bind` + `fixed_channel`、メタ `profile: "osc_udp"`。アドレス前置フィルタは graph 側 `motion.filter` 等で）
- [x] VMC Protocol pose send（`/VMC/Ext/Bone/Pos` / `/VMC/Ext/Root/Pos` の単発送出ノード + `flowgraph::vmc` ヘルパ。全骨ストリームはグラフ側ループ）
- [x] VMC Protocol pose recv（UDP 受信は既存 `ingress.vmc_udp` / `ingress.osc_udp` + `motion.vmc_parse`。**抽出**: `flowgraph.vmc.extract_bone_pos` / `extract_root_pos` が `MotionFrame` から `/VMC/Ext/*/Pos` を JSON 化）
- [x] VRChat OSC 専用ヘルパー（`flowgraph::vrchat` + `flowgraph.vrchat.avatar_parameter_{float,int,bool}` / `chatbox_input` / `chatbox_typing`。公式 OSC ドキュメント準拠の固定アドレス）
- scope: iFacialMocap 単独ノードは VMC bridge 経由で吸収できる前提で外す。足りなければ長期 backlog に再掲

### Phase σ — 外部連携 HTTP + OBS + System Metrics + Twitch Helix 拡張 (TBD)

外部 API 系の横串拡張フェーズ。既存 `[src/flowgraph/nodes/twitch.rs](../src/flowgraph/nodes/twitch.rs)` の Helix / OAuth 基盤を流用しつつ、HTTP 汎用ノード / OBS WebSocket / system metrics を同じ phase に詰める。

- [ ] `flowgraph.http.request`（GET/POST/PUT/DELETE/PATCH、headers / JSON body / timeout / status / body / retry policy）
- [ ] `flowgraph.obs.*`（`obws` crate 想定、scene switch / source visibility / record start-stop / stream start-stop / current scene / studio mode transition）
- [ ] `flowgraph.system.*`（`sysinfo` crate、cpu_usage / mem_used / mem_total / load_avg / process_list。GPU は NVML 依存で後回し）
- [ ] `flowgraph.twitch.*` 拡張（ユーザ要求分）: `raid_start` / `raid_cancel` / `ad_run`（1 分広告）/ `chat_settings_update`（subscribers_only / followers_only / emote_only / slow / unique）/ `prediction_create` / `prediction_end` / `poll_create` / `poll_end` / `shield_mode_update`（防御モード）/ `stream_marker_create`（説明付き対応）/ `clip_create` / `channel_info_update` / `goals_get` / `chat_clear` / チャット履歴リフレッシュ
- open question: ユーザ要求の「RAID を 1 時間停止する」は Twitch 側に 1:1 の Helix エンドポイントが無く、`blocked_terms` 運用か独自 state で "incoming raid 遮断" を表現する必要あり → phase doc 内で TBD として扱う

### Phase τ — Process / Window 制御 (TBD)

OS プロセス / ウィンドウ制御ノード群。Windows を第一級 target、他 OS は degrade policy を phase doc で固定する。

- [ ] `flowgraph.process.spawn` / `.kill` / `.wait` / `.running`（PID / exe 名で条件判定）
- [ ] `flowgraph.window.enum`（現在開いているウィンドウ一覧を Table で返す）
- [ ] `flowgraph.window.move` / `.resize` / `.minimize` / `.maximize` / `.restore` / `.close` / `.foreground`
- [ ] `flowgraph.window.pseudo_fullscreen` / `.pseudo_fullscreen_exit`（borderless + monitor-size 化 / 元サイズ復帰）
- scope: 既存 `windows` crate を再利用。macOS / Linux は no-op + warn か、将来別 backend を追加するかを phase doc で決める

### Phase υ — GUI 大物 (Undo/Redo + multi-select + subgraph) (TBD)

Flowgraph editor の大規模 UX 改修。ν-β で送った "Svelte Flow handle drag edge の E2E" もここに合流させ、履歴モデルを第一級概念化する。

- **λ との分担**: 再利用の**意味論**（閉集合、`library_uses`、境界ノード v0）は [Phase λ（完了）](roadmap/phase-lambda-flowgraph-enum-and-library.md)。υ の subgraph / グループは **エディタ上のカプセル化・Undo 等**が主で、将来の engine 側合成は λ（λ+）の境界モデルと整合させる。

- [ ] 汎用 Undo/Redo スタック（現状 "削除 1 段 snapshot" を command pattern に進化、add/delete/move/connect/disconnect/property-edit 全部対象）
- [ ] 本物のマルチ選択（`selectedNodeIds: Set<string>` + 矩形選択 + shift-click + ctrl-click、property editor multi 表示 / 差異ハイライト）
- [ ] Flowgraph subgraph / group（engine + GUI の両面で第一級概念化、入出力 port を再 export するカプセル化）
- [ ] Svelte Flow handle drag edge の E2E 回帰（ν-β+ から昇格）

### Phase ω — Audio-reactive + Physics (TBD)

Phase ο の vec2/3 と signal util に直接乗る形で、音声反応と古典力学系を追加する。procedural avatar motion のコア。

- [ ] `flowgraph.audio.play`（`rodio` crate 想定、SE ファイル再生 / 音量 / ピッチ）
- [ ] `flowgraph.audio.envelope`（voice ingress 副産物から RMS / ピーク / 平滑化）
- [ ] `flowgraph.audio.pitch`（voice ingress の fundamental frequency 抽出）
- [ ] `flowgraph.physics.spring`（target + stiffness + damping + velocity state）
- [ ] `flowgraph.physics.damper` / `.integrator` / `.gravity`（vec2/vec3 ベース）
- scope: avatar renderer を持たない前提で、OSC / VMC 経由で外部 renderer に流すことを想定

### Unscheduled Flowgraph Nodes

- **Phase λ（v0 完了）**: `PortSpec.closed_string_variants`、`[[enums]]`、`flowgraph.library.input` / `output`（スタブ）、`FileMeta` 拡張 + `library_uses` 閉路検出。解説: [`manual/flowgraph-enum-and-library.md`](manual/flowgraph-enum-and-library.md)。動的境界ポート等の **λ+** は [`phase-lambda-flowgraph-enum-and-library.md`](roadmap/phase-lambda-flowgraph-enum-and-library.md) 参照。
- **Phase π**: `flowgraph.datetime.*` 8 ノード（[`phase-pi-datetime-system.md`](roadmap/phase-pi-datetime-system.md) / [`manual/datetime-system.md`](manual/datetime-system.md)）**実装済み**。
- **Phase ο-4（実装済み）**: `flowgraph.util.timer_interval`（[`roadmap/backlog-nodes.md`](roadmap/backlog-nodes.md) §1、`phase-omicron` §3.4、[`src/flowgraph/nodes/timer_interval.rs`](../src/flowgraph/nodes/timer_interval.rs)）

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

ψ-α / ν / ν-β / Phase ο 以降の Flowgraph 拡張と並行、または一段落した後に検討（ユーザー意向として "GUI の Tauri 化" を積んでいる）。**実装順**: 再構造化 → CLI + desktop の 2 実行ファイル → desktop へ Tauri を載せる（§3 先頭、[`v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md) §1.1）。仕様書: [`roadmap/phase-epsilon-shutdown-and-tauri.md`](roadmap/phase-epsilon-shutdown-and-tauri.md)

### 長期 backlog（phase 立て前の候補リスト）

設計重量が大きい or VAC 現役ユースケースへの直結度を需要確認してから phase 化する候補群。各 1 行のみ、詳細は phase doc 化のタイミングで起こす。

- [ ] **Discord voice ingress**（Discord bot + voice gateway + opus decode、大工事。独立 phase / または VAC とは別プロセスの bridge 化も視野）
- [ ] **iFacialMocap 単独**（ρ の VMC bridge 経由で吸収できない場合のみ。需要次第）
- [ ] **VTube Studio API**（WebSocket、表情 / パラメータ / Hotkey 制御。OSC と機能重複するため ρ 完了後に需要を再確認）
- [ ] **Global hotkey / MIDI**（StreamDeck 互換、OS 横断の global hotkey listener + MIDI input ingress ノード）
- [ ] **独自 avatar renderer**（VAC が OSC / VMC 経由で外部 renderer を制御する現行路線に対して、独自に renderer を内包する巨大フェーズ。ψ / η 規模、別軸）

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
- [`roadmap/v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md) — VMC 対応と再構造化（Phase **M0〜M5**）
- [`roadmap/phase-mu-vmc-motion-m0.md`](roadmap/phase-mu-vmc-motion-m0.md) — motion 層 M0（VMC UDP パススルー）詳細
- [`roadmap/phase-delta-spec.md`](roadmap/phase-delta-spec.md) — Flowgraph 基幹仕様
- [`manual/index.md`](manual/index.md) — ユーザー向けマニュアル
- [`../CHANGELOG.md`](../CHANGELOG.md) — Phase 単位の変更履歴
