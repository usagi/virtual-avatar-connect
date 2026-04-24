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

### Phase ξ — Dimensional Quantity System (SI 準拠の単位次元システム)

Flowgraph engine に **SI 準拠の単位次元システム**を第一級概念として導入する基盤フェーズ。数値に unit を付与、unit は次元（L·M·T·I·Θ·N·J + 疑似次元 Angle の 8 成分）を持つ。strict default（次元不一致は engine error）+ 明示 escape hatch（`flowgraph.unit.strip`）の方針。IO 系ノードは pass-through（wire format 互換を死守、次元強制は個別 opt-in）。既存 flow は dimensionless fallback で完全後方互換。

Flowgraph は pure-functional + 遅延評価のため runtime 単位評価コストが DAG 枝刈り / memoization で自然に償却される — これが「工学系出身者が自作アプリに求める単位安全性」を現実的コストで提供できる根拠（詳細 [`roadmap/phase-ksi-dimensional-quantity-system.md`](roadmap/phase-ksi-dimensional-quantity-system.md) §1.1）。本フェーズは **Phase ο (Flowgraph Enhancement I) の順序上の前提**。

- [x] ξ-0 docs: `phase-ksi-dimensional-quantity-system.md` 新設 + roadmap.md への Phase ξ 追加 + Phase ο doc の依存注記
- [x] ξ-1 feat(flowgraph/quantity): `Dimension` / `Unit` / `Quantity` 型 + SI 基本 7 単位 + 主要誘導単位 + SI 接頭辞 20 種 + Angle 疑似次元（rad / deg）+ 温度 delta 分離 (K / ΔK) + unit 文字列 parser + unit test 30+
- [x] ξ-2 feat(flowgraph/nodes/unit): `flowgraph.unit.*` 操作ノード 7 種（assign / convert / strip / get_unit_string / get_dim_string / same_dimension / to_json）+ `SocketType::Quantity` / `SocketValue::Quantity` 追加 + TOML / JSON wire format 対応 + `Unit::to_si_base` atom-canonical 係数込み
- [x] ξ-3 refactor(flowgraph): engine 側に `Float ↔ Quantity` 暗黙 coerce を新設（`SocketType::compatible_with` / `coerce_to_type`）。`flowgraph.math.float_*` 4 種を Quantity 演算化（`try_add` / `try_sub` / `try_mul` / `try_div`、div-by-zero / 次元不一致は明示エラー）。既存フローは dimensionless fallback で完全後方互換
- [x] ξ-4 feat(flowgraph/util): engine 側に `Quantity → String` 暗黙 coerce 追加（Display 実装経由、一方向のみ）。`flowgraph.util.format` 新設（`include_unit` / `precision` / `unit_override` プロパティ）。`flowgraph.util.log` / `flowgraph.channel.emit` は port 型そのままで stringify が unit-aware に
- [ ] ξ-5 feat(gui): `FlowgraphNodeCard.svelte` ポート chip に unit バッジ + Dimension family 色分け + hover tooltip + property editor の unit text input
- [x] ξ-6 docs: CHANGELOG + `docs/manual/dimensional-quantity-system.md` 新設（ユーザ向け解説: 動機 / 使える単位 / parser / ノード紹介 / よくあるパターン / FAQ）+ `manual/index.md` 目次 + Socket 型列に `quantity` / `table` 追記 + roadmap tick
- 仕様書: [`roadmap/phase-ksi-dimensional-quantity-system.md`](roadmap/phase-ksi-dimensional-quantity-system.md)
- scope: 外部依存ゼロ（自作、`uom` crate は runtime vs compile-time の性質不一致で採用見送り）。既存 flow 完全後方互換（dimensionless fallback）。IO 系は pass-through。非対応: Celsius/Fahrenheit（ξ+）/ 非 rad-deg Angle 単位 / ユーザ定義次元 / GUI unit インライン編集（τ 合流候補）
- 順序: **本フェーズ着地後に Phase ο-1 着手**。math / easing / vec / time / signal util ノード群は最初から `Quantity<Dimension>` 対応で実装する

---

### Phase ο — Flowgraph Enhancement I (計算系 + 時間 + signal util + GUI 小改善)

Flowgraph 機能向上の第 1 波。engine 内完結の Pure ノード群（math 拡張 / easing / vec2・vec3 / time・timer / signal util / random・noise、計 **78 ノード**: 42+1+20+5+5+5 — ο-0 の「76 (vec 18)」は `add/sub` 行数ミスで ο-3 実装時に 20 に修正）と GUI 小改善 3 項目（palette カテゴリ絞り込み / canvas drop-at-cursor / Ctrl+D duplicate）を narrow scope で追加する。外部 IO（HTTP / OBS / OSC / VMC / system metrics / process / window / Discord voice 等）と engine 大改修（Undo/Redo / subgraph / 物理）は明示的に次フェーズ（π / ρ / σ / τ / υ）以降へ送る。Phase ξ (Dimensional Quantity System) の ξ-1〜ξ-4/ξ-6 着地済み → **ο-1 / ο-2 / ο-3 着地済み**、ο-4 以降は ξ-5 GUI と並行進行可。

- [x] ο-0 docs: `phase-omicron-flowgraph-enhancement.md` 新設 + roadmap.md の Active 差し替え + backlog-nodes.md §1 を ο-4 昇格 pointer 化（+ 追補: angle normalization 4 / 双曲線 6 ノード追加 + Phase ξ 依存明記）
- [x] ο-1 feat(flowgraph/math): §3.1 **42 ノード**追加（abs/min/max/clamp/lerp/inverse_lerp/remap/smoothstep/trig/arctrig/atan2/hyperbolic/arc-hyperbolic/sqrt/pow/exp/log/sign/floor/ceil/round/deg↔rad/normalize_angle × 4）。全 Quantity-aware、30 unit tests 全緑、node-catalog.md 再生成済み。ο-0 docs の「37」表記は実装時に 42 へ正確化
- [x] ο-2 feat(flowgraph/easing): §3.2 `flowgraph.easing.apply` + curve enum 19 種 (linear + quad/cubic/sine/expo/elastic/bounce × in/out/inOut) + `PropertySpec.choices` 追加 + GUI `FlowgraphPropertyEditor` の `<select>` dropdown hook。13 unit tests 全緑、node-catalog.md 再生成済み
- [x] ο-3 feat(flowgraph/vec): §3.3 vec2 / vec3 × 10 ノード ✕ 2 = **20 ノード**（make/unpack/add/sub/scale/dot/length/normalize/lerp/distance、JSON 配列表現）。`decode_vec::<N>` / `encode_vec::<N>` の `const N: usize` generic helper + 6 種マクロで型安全に実装、19 unit tests 全緑（round-trip / 零ベクトル normalize / 次元不一致 error / 非有限値 reject 含む）、node-catalog.md 再生成済み
- [ ] ο-4 feat(flowgraph/util,time): §3.4 `flowgraph.util.timer_interval`（backlog §1 から昇格）+ `flowgraph.time.*` 4 種（now_rfc3339 / now_epoch_ms / format / since_ms）
- [ ] ο-5 feat(flowgraph/util,random,noise): §3.5 signal util 5 種（edge_detect / prev_value / sample_hold / debounce / throttle）+ §3.6 random 3 種 + Perlin 1D/2D。`noise` crate 追加
- [ ] ο-6 feat(gui): §3.7 palette カテゴリ絞り込みトグル + canvas drop-at-cursor + Ctrl+D duplicate + `flowgraph-canvas-basic.spec.ts` 回帰拡充
- [ ] ο-7 docs: CHANGELOG + `manual/node-catalog.md` 再生成 + roadmap tick
- 仕様書: [`roadmap/phase-omicron-flowgraph-enhancement.md`](roadmap/phase-omicron-flowgraph-enhancement.md)
- scope: narrow-scoped。engine 内完結 Pure ノード + GUI 小改善に閉じる。新規 crate 依存は `noise` 1 件のみ。Breaking change なし

---

## Backlog / Future

### Phase π — OSC / VMC / VRC bridge (TBD)

Flowgraph から OSC（Open Sound Control）を使ってアバターアプリ・VRChat・その他 OSC 対応ソフト（VTube Studio の一部 / LiveLinkFace 等）を制御する基盤。VAC を「独自 avatar renderer を持つ前に、既存アバターアプリを Flowgraph から総合制御するハブ」に格上げする phase。

- [ ] `rosc` crate 追加 + `src/flowgraph/osc.rs` 基盤（UDP sender / receiver の ingress 型）
- [ ] `flowgraph.osc.send`（address / args JSON / host / port）
- [ ] `flowgraph.ingress.osc`（bind port + address filter → exec + args 展開）
- [ ] VMC Protocol pose send（アバター姿勢データを VMC プロトコル準拠 OSC で送出）
- [ ] VMC Protocol pose recv（iFacialMocap / 各種トラッカーからの VMC 受信 ingress）
- [ ] VRChat OSC 専用ヘルパー（avatar param set / chatbox send / typing indicator）
- scope: iFacialMocap 単独ノードは VMC bridge 経由で吸収できる前提で外す。足りなければ長期 backlog に再掲

### Phase ρ — 外部連携 HTTP + OBS + System Metrics + Twitch Helix 拡張 (TBD)

外部 API 系の横串拡張フェーズ。既存 `[src/flowgraph/nodes/twitch.rs](../src/flowgraph/nodes/twitch.rs)` の Helix / OAuth 基盤を流用しつつ、HTTP 汎用ノード / OBS WebSocket / system metrics を同じ phase に詰める。

- [ ] `flowgraph.http.request`（GET/POST/PUT/DELETE/PATCH、headers / JSON body / timeout / status / body / retry policy）
- [ ] `flowgraph.obs.*`（`obws` crate 想定、scene switch / source visibility / record start-stop / stream start-stop / current scene / studio mode transition）
- [ ] `flowgraph.system.*`（`sysinfo` crate、cpu_usage / mem_used / mem_total / load_avg / process_list。GPU は NVML 依存で後回し）
- [ ] `flowgraph.twitch.*` 拡張（ユーザ要求分）: `raid_start` / `raid_cancel` / `ad_run`（1 分広告）/ `chat_settings_update`（subscribers_only / followers_only / emote_only / slow / unique）/ `prediction_create` / `prediction_end` / `poll_create` / `poll_end` / `shield_mode_update`（防御モード）/ `stream_marker_create`（説明付き対応）/ `clip_create` / `channel_info_update` / `goals_get` / `chat_clear` / チャット履歴リフレッシュ
- open question: ユーザ要求の「RAID を 1 時間停止する」は Twitch 側に 1:1 の Helix エンドポイントが無く、`blocked_terms` 運用か独自 state で "incoming raid 遮断" を表現する必要あり → phase doc 内で TBD として扱う

### Phase σ — Process / Window 制御 (TBD)

OS プロセス / ウィンドウ制御ノード群。Windows を第一級 target、他 OS は degrade policy を phase doc で固定する。

- [ ] `flowgraph.process.spawn` / `.kill` / `.wait` / `.running`（PID / exe 名で条件判定）
- [ ] `flowgraph.window.enum`（現在開いているウィンドウ一覧を Table で返す）
- [ ] `flowgraph.window.move` / `.resize` / `.minimize` / `.maximize` / `.restore` / `.close` / `.foreground`
- [ ] `flowgraph.window.pseudo_fullscreen` / `.pseudo_fullscreen_exit`（borderless + monitor-size 化 / 元サイズ復帰）
- scope: 既存 `windows` crate を再利用。macOS / Linux は no-op + warn か、将来別 backend を追加するかを phase doc で決める

### Phase τ — GUI 大物 (Undo/Redo + multi-select + subgraph) (TBD)

Flowgraph editor の大規模 UX 改修。ν-β で送った "Svelte Flow handle drag edge の E2E" もここに合流させ、履歴モデルを第一級概念化する。

- [ ] 汎用 Undo/Redo スタック（現状 "削除 1 段 snapshot" を command pattern に進化、add/delete/move/connect/disconnect/property-edit 全部対象）
- [ ] 本物のマルチ選択（`selectedNodeIds: Set<string>` + 矩形選択 + shift-click + ctrl-click、property editor multi 表示 / 差異ハイライト）
- [ ] Flowgraph subgraph / group（engine + GUI の両面で第一級概念化、入出力 port を再 export するカプセル化）
- [ ] Svelte Flow handle drag edge の E2E 回帰（ν-β+ から昇格）

### Phase υ — Audio-reactive + Physics (TBD)

Phase ο の vec2/3 と signal util に直接乗る形で、音声反応と古典力学系を追加する。procedural avatar motion のコア。

- [ ] `flowgraph.audio.play`（`rodio` crate 想定、SE ファイル再生 / 音量 / ピッチ）
- [ ] `flowgraph.audio.envelope`（voice ingress 副産物から RMS / ピーク / 平滑化）
- [ ] `flowgraph.audio.pitch`（voice ingress の fundamental frequency 抽出）
- [ ] `flowgraph.physics.spring`（target + stiffness + damping + velocity state）
- [ ] `flowgraph.physics.damper` / `.integrator` / `.gravity`（vec2/vec3 ベース）
- scope: avatar renderer を持たない前提で、OSC / VMC 経由で外部 renderer に流すことを想定

### Unscheduled Flowgraph Nodes

（Phase ο-4 で `flowgraph.util.timer_interval` は昇格済み。詳細は [`roadmap/backlog-nodes.md`](roadmap/backlog-nodes.md)）

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

ψ-α / ν / ν-β / Phase ο 以降の Flowgraph 拡張と並行、または一段落した後に検討（ユーザー意向として "GUI の Tauri 化" を積んでいる）。仕様書: [`roadmap/phase-epsilon-shutdown-and-tauri.md`](roadmap/phase-epsilon-shutdown-and-tauri.md)

### 長期 backlog（phase 立て前の候補リスト）

設計重量が大きい or VAC 現役ユースケースへの直結度を需要確認してから phase 化する候補群。各 1 行のみ、詳細は phase doc 化のタイミングで起こす。

- [ ] **Discord voice ingress**（Discord bot + voice gateway + opus decode、大工事。独立 phase / または VAC とは別プロセスの bridge 化も視野）
- [ ] **iFacialMocap 単独**（π の VMC bridge 経由で吸収できない場合のみ。需要次第）
- [ ] **VTube Studio API**（WebSocket、表情 / パラメータ / Hotkey 制御。OSC と機能重複するため π 完了後に需要を再確認）
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
- [`roadmap/phase-delta-spec.md`](roadmap/phase-delta-spec.md) — Flowgraph 基幹仕様
- [`manual/index.md`](manual/index.md) — ユーザー向けマニュアル
- [`../CHANGELOG.md`](../CHANGELOG.md) — Phase 単位の変更履歴
