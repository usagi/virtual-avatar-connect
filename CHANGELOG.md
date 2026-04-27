# Changelog

本ファイルは VAC (Virtual Avatar Connect) の主要な変更履歴を Phase 単位で記録する。
個別の詳細設計は `docs/roadmap/phase-delta-spec.md` を参照。

## [Unreleased]

### M-4a / Step 7 / ρ 先取り — motion OSC パース、OSC 送信、fixture runner、`app_core`

- **`rosc` 依存** + **`src/motion/frame.rs`** / **`vmc_osc.rs`**: UDP ペイロードの OSC デコード → JSON（`byte_len` / `osc_messages[]`）。
- **`flowgraph.motion.vmc_parse`**（Pure）: `payload_b64` → `frame`（Json）。
- **`flowgraph.motion.filter`**（Pure）: `vmc_parse` の `frame` から `osc_messages` を `address_prefix` / `address_substring` で絞り込み。
- **`flowgraph.ingress.osc_udp`** + **`bridges::osc_ingress`**: 汎用 OSC/UDP ingress（`__meta__.profile = "osc_udp"`）。VMC ingress（`vmc_udp`）とメタ分離。
- **`flowgraph.osc.send`**（Effectful）: `host` / `port` / `path` / `args`（JSON 配列）で単発 OSC を UDP 送信、`on_success` / `on_error`。
- **`src/flowgraph/fixture_runner.rs`**: `load_fixture_program` / `load_and_execute_once` + `flowgraph.example/lambda-demo` の lib テスト。
- **`src/app_core.rs`**: `run_vac_application` + actix `run_services` を `lib::run` から分離（再構造化 Step 7 一段）。
- **`lib.rs`**: `Arc` / `RwLock` をクレート根で `pub use`（既存 `crate::RwLock` パスを維持）。
- **`docs/manual/node-catalog.md`**: 新ノード反映。

### RM-1 — `conf.toml` に `[modes.*]`（Runtime Mode 宣言）を追加

- **`src/conf/runtime_mode.rs`**: `modes` / `default_runtime_mode` の serde 用型とロード時検証（Managed App ID と `run_with` の整合、enable/disable の重複禁止）。
- **`Conf::load`**: 上記検証をパース直後に実行（`new_noop_probe` 経由の事前確認にも効く）。
- **`docs/manual/conf-reference.md`**: §3.2 を追加。

### RM-3（一部）— Flowgraph `[meta]` の mode 用メタとローダ出力

- **`FileMeta`**: `mode_groups` / `default_enabled`（省略時 `true`）。空のグループ名はロードエラー（`DiagnosticCode::InvalidModeMetadata`）。
- **`LoadReport` / `FlowgraphRuntime`**: ファイル fq → `FlowgraphFileActivationMeta` の `file_activation`。
- **`GET .../flowgraph/diagnostics`**: レスポンスに `file_activation` を追加。
- **`docs/manual/flowgraph-enum-and-library.md`**: RM-3 節を追加。

### RM-3（続き）— exec 経路の活性と trigger 抑止

- **`src/flowgraph/activation.rs`**: `conf` + `file_activation` + ノードメタから exec 活性を解決する `TriggerGate`（`default_runtime_mode` と `[modes.*].flowgraph_groups` を使用）。
- **`FlowgraphProgram::fire_node`**: ゲートで非活性のノードは exec 処理をスキップ（pull 由来の Pure/Stateful 評価は従来どおり）。
- **`run_forever_with_bus`**: 任意の `TriggerGate` を `ExecCtx` に渡せるよう拡張。
- **`FlowgraphRuntime::load_and_spawn`**: `conf: Option<&Conf>` を受け取りワーカーにゲートを渡す。`State::new` は `Some(conf)`、reload は `conf_source_path` から再読込。
- **診断 JSON**: `inactive_exec_nodes` を追加。

### RM-2 / RM-3（続き）— Runtime Mode の整合・Control API・Pure での観測

- **`orphan-mode-group`**: `[meta].mode_groups` の名前がいずれの `[modes].*.flowgraph_groups` にも出ない場合に warning 診断（`OrphanModeGroup`）。
- **`State.runtime_mode_id`**: 現在の mode 上書き（`None`＝`default_runtime_mode` 相当）。`PUT /api/v1/control/modes/current` で更新し **`TriggerGate::recompute`** で exec 抑止を即時再計算。
- **Control API**: `GET/PUT /api/v1/control/modes`、`GET .../modes/current`（`conf_source_path` 必須）。
- **`PureEvalHost`**: `PureNode::compute` に engine から渡す隻参照（`runtime_mode` 共有スロット + `default_runtime_mode`）。`flowgraph.mode.get` / `flowgraph.mode.equals`（観測ノード、RM-2 節 `runtime-mode-roadmap.md` 対応）。

### RM-5（一部）— plan / transit dry-run / WS / Flowgraph transit ノード

- **`POST /api/v1/control/modes/plan`**: `ModeTransitionPlan`（実効 ID、noop、target の flowgraph_groups / managed_apps、enable/disable の宣言差分プレビュー）。
- **`POST /api/v1/control/modes/transit`**: `dry_run` で再読込 conf のみ返却、本適用時は `plan` を同梱。
- **`ControlEvent::RuntimeModeChanged`**: `/control/events` WebSocket に配信（実変化時のみ）。
- **`state::apply_runtime_mode_change`**: Control API と共有。
- **`flowgraph.mode.transit`**: Effectful ノード（`control_triggerable` は false）。

### RM-5（続き）— Managed App 適用・遷移ロック・Flowgraph 内部 trigger

- **非 noop の遷移**（`PUT /modes/current` / `POST /modes/transit` の本適用 / `flowgraph.mode.transit`）では `State.runtime_mode_transition_busy` で再入を拒否（HTTP 409 / ノードは `transition_busy`）。
- **`apply_runtime_mode_transition_full`**: `apply_runtime_mode_change` の後に、実効 mode の `[modes.*].managed_apps` を stop → start → minimize の順で best-effort 適用。
- **`ControlEvent::RuntimeModeManagedApps`**: 上記の操作ログを WS に配信（1 件以上のとき）。
- **API 応答**: `PUT .../current` と `POST .../transit`（本適用）に任意フィールド `managed_apps`（操作行の配列）。
- **`[flowgraph].runtime_mode_changed_trigger_node_id`**: 実効 mode が変わった直後に、指定 fq ノードへ ingress 互換の `TriggerEvent`（`__content__` / `__meta__` に JSON）を 1 発投入。

### RM-5（quiesce）— 遷移中の Flowgraph exec 一時抑止

- **`TriggerGate::global_exec_suppress`**: `true` の間は RM-3 の per-node マップに関わらず **すべてのノード**で exec を抑止（`engine::fire_node` 先頭の `is_exec_active` 経由）。
- **`apply_runtime_mode_transition_full`**: 実効変化が確実なときだけ suppress を立て、スロット更新〜 Managed App 適用の後・`runtime_mode_changed` 内部フックの直前に解除（フックは常に通る）。
- **`ApplyRuntimeModeError::PlanFailed`**: 遷移本体の plan 再検証失敗時。Control API は `plan_failed` と `unknown_mode` を区別。

### 内部リファクタ — Control API `actions` / `dto` / `ping` / `shutdown` / `ingress` モジュール分割

- **`src/web_interface/control/actions/`**: 旧 `actions.rs` を `mod.rs`（snapshot・`PauseTarget`・ルート）と `pause.rs`（pause/resume 適用・`resolve_ai_index`・単体テスト）に分割。
- **`src/web_interface/control/dto/`**: 旧 `dto.rs` を `mod.rs`（snapshot DTO・`snapshot`）と `twitch_summary.rs`（`compute_twitch_authorized`）に分割。先頭 rustdoc の文字化けを修正。
- **`src/web_interface/control/ping/`**: 旧 `ping.rs` を `mod.rs` と `responses.rs`（`Pong` / `WhoAmI`）に分割。
- **`src/web_interface/control/shutdown/`**: 旧 `shutdown.rs` を `mod.rs`（ハンドラ）と `types.rs`（DTO・serde テスト）に分割。
- **`src/web_interface/control/ingress/`**: 旧 `ingress.rs` を `mod.rs`（DTO・`configure`）と `post.rs`（`POST /ingress`）に分割。

### 内部リファクタ — Control API `bos` / `reload` / `ws` モジュール分割

- **`src/web_interface/control/bos/`**: 旧 `bos.rs` を `mod.rs`（DTO・`GET /bos`）と `util.rs`（カテゴリ推定・channel 検出・タイトル・単体テスト）に分割。
- **`src/web_interface/control/reload/`**: 旧 `reload.rs` を `mod.rs`（`ReloadRequest` / ルート）と `ai_persona.rs`（`handle_ai_reload` と JSON エラー応答）に分割。
- **`src/web_interface/control/ws/`**: 旧 `ws.rs` を `mod.rs`（`events_ws`）と `actor.rs`（`ControlEventsWs` と配信ループ）に分割。

### 内部リファクタ — Control API `auth` / `oauth_twitch` / `managed_app` モジュール分割

- **`src/web_interface/control/auth/`**: 旧 `auth.rs` を `mod.rs`、`runtime.rs`（`ControlApiRuntime` / `TokenSource` / トークン解決）、`middleware.rs`（`control_api_auth` と結合・単体テスト）に分割。モジュール先頭の rustdoc を日本語で整理。
- **`src/web_interface/control/oauth_twitch/`**: 旧 `oauth_twitch.rs` を `mod.rs`（ハンドラ・`OAuthStartResponse`）と `start_flow.rs`（Device Code Flow の開始とポーリングタスク登録）に分割。
- **`src/web_interface/control/managed_app/`**: 旧 `managed_app.rs` を `mod.rs`（DTO・ハンドラ）と `lookup.rs`（`lookup_entry` / `lookup_spec_status`）に分割。

### 内部リファクタ — Control API `restart` / `run_with` モジュール分割

- **`src/web_interface/control/restart/`**: 旧 `restart.rs` を `mod.rs`（ハンドラ・DTO）と `util.rs`（`resolve_new_conf_path` / `label_from_filename` / `paths_equivalent` と単体テスト）に分割。
- **`src/web_interface/control/run_with/`**: 旧 `run_with.rs` を `mod.rs`（DTO・`mutate_conf`・ハンドラ）、`util.rs`（`err_response` / `build_views`）、`toml_ops.rs`（`run_with_array` / `dto_to_value` と単体テスト）に分割。

### 内部リファクタ — Control API `profiles` モジュール分割

- **`src/web_interface/control/profiles/`**: 旧 `profiles.rs` を `mod.rs`（DTO・ハンドラ）と `util.rs`（パス検証・バックアップ・`is_current`、単体テスト）に分割。

### 内部リファクタ — Control API `table` モジュール分割

- **`src/web_interface/control/table/`**: 旧単一 `table.rs` を `mod.rs`（DTO・ハンドラ・結合テスト）と `util.rs`（allow-list / If-Match / TSV I/O・行ロック）に分割。

### 内部リファクタ — Control API `flowgraph` モジュール分割

- **`src/web_interface/control/flowgraph/`**: 旧単一 `flowgraph.rs` を `mod.rs`（ツリー・ファイル CRUD・reload）、`util.rs`（パス安全・node-catalog ヒント）、`reload.rs`（ランタイム再構築 + WS）、`trigger.rs`（外部トリガ）、`fragment_zip.rs`（fragment / ZIP）に分割。`reload_runtime` は引き続き `flowgraph::reload_runtime` で `pub(crate)` 公開。

### 内部リファクタ — Control イベント型

- **`src/control_events.rs`**: `ControlEvent` / `ChannelDatumPhase` / `ProcessorInvocationOutcome` を定義。`state` は **`web_interface` に依存しない**（`from_channel_datum` は `state` 内の `impl ControlEvent`）。[`docs/architecture.md`](docs/architecture.md) の境界表を追随。

### 内部リファクタ — Twitch OAuth セッション

- **`src/twitch_oauth_sessions.rs`**: `OAuthSessions` / `OAuthSessionView` 等を `web_interface` から分離。[`docs/architecture.md`](docs/architecture.md) の境界表を追随。

### v2 Step 4（一段）— モジュール境界の文書化

- **[`docs/architecture.md`](docs/architecture.md)**: 「レイヤ境界（Step 4）」表（`motion` / `bridges` / `flowgraph` / `web_interface` / `state` と `state→web_interface` 例外）。
- **Rustdoc**: [`src/motion/mod.rs`](src/motion/mod.rs)、[`src/bridges/mod.rs`](src/bridges/mod.rs)、[`src/state/mod.rs`](src/state/mod.rs) に依存契約・逆流メモ。
- **[`docs/roadmap/v2-vmc-and-restructure.md`](docs/roadmap/v2-vmc-and-restructure.md)** Step 4、[`docs/roadmap.md`](docs/roadmap.md) v2 メタ。

### ドキュメント（v2 CI 方針メモ）

- **部分 CI は入れない**: [`docs/roadmap/v2-vmc-and-restructure.md`](docs/roadmap/v2-vmc-and-restructure.md) §1.3。Step 6b / CI はワークスペース確定後にまとめて設計。[`docs/roadmap.md`](docs/roadmap.md) v2 メタの Step 6b 表記を整合。

### v2 Step 6b — `vac-gui-assets` ワークスペースメンバ

- **クレート** [`vac-gui-assets/`](vac-gui-assets/): `include_dir` + `build.rs`（`gui/dist/index.html` 必須）。`embed-gui` 時にメイン crate から依存。
- **ワークスペース**: ルート [`Cargo.toml`](../Cargo.toml) に `[workspace]`（`members = [".", "vac-gui-assets"]`、`default-members = ["."]`）。既定 `cargo test` は `vac-gui-assets` をビルドしない。
- **メイン**: `embed-gui` は `dep:vac-gui-assets` のみ（ルート `build.rs` の GUI 検査は削除）。
- **ドキュメント**: v2 §3 図・Step 5/6、[`docs/roadmap.md`](docs/roadmap.md) v2 メタ。

### v2 Step 6a — GUI `embed-gui`（バイナリ内蔵 `/gui/*`）

- **Cargo feature `embed-gui`**: `/gui/*` をメモリから配信（[`src/web_interface/gui_embedded.rs`](../src/web_interface/gui_embedded.rs)）。GUI バイト列は **`vac-gui-assets`** に集約（Step 6b）。
- **パス正規化**: [`src/web_interface/gui_path.rs`](../src/web_interface/gui_path.rs)（`..` 排除のユニットテスト）。

### ドキュメント（実行形態・工程順）

- **desktop / CLI / Tauri の順序**: [`docs/architecture.md`](docs/architecture.md) に「実行入口（計画・工程順）」を追加。[`docs/roadmap/v2-vmc-and-restructure.md`](docs/roadmap/v2-vmc-and-restructure.md) §1.1（実行形態・desktop 命名理由・トレイ UX 目標）、[`docs/roadmap/phase-epsilon-shutdown-and-tauri.md`](docs/roadmap/phase-epsilon-shutdown-and-tauri.md) §3 / §4.3、[`docs/roadmap.md`](docs/roadmap.md) Phase ε を整合。
- **GUI のリリース時内蔵**（`npm run dev` 不要）: v2 §1.2、architecture（Scope・実行入口）、phase-epsilon §3.6。
- **再構造化の追跡**: v2 §3 に `vac-gui-assets` と移行手順 Step 4〜9（進捗メモ・Step 1〜3 完了表記）、[`roadmap.md`](roadmap.md) に「v2 crate / runner / GUI 同梱」チェックリスト。

### Phase M2 — VMC パススルー・ハブ運用（conf / ログ）

- **`[[motion.vmc_passthrough]]`**: 任意の `label`（ログ識別。空なら `bind` を文脈に使用）。同一 `bind` を複数エントリが参照する場合は起動時 `warn`。
- **`src/motion/router.rs`**: `send_to` 失敗ログを `SendFailLogThrottle` で間引き（約 5 秒に最大 1 回のまとめ + 省略件数）。
- **ドキュメント**: [`docs/roadmap/phase-mu-vmc-motion-m0.md`](docs/roadmap/phase-mu-vmc-motion-m0.md) §9、[`docs/manual/conf-reference.md`](docs/manual/conf-reference.md)、[`conf.example-motion.toml`](conf.example-motion.toml)（複数受信口の例）。

### Phase M1 — `flowgraph.ingress.vmc_udp` + VMC UDP ブリッジ

- **ノード**: `flowgraph.ingress.vmc_udp`（ingress echo パターン）。`properties.bind`（`host:port`）で UDP 受信。`content` は受信ペイロードの **Base64**。
- **ブリッジ**: [`src/bridges/vmc_ingress.rs`](src/bridges/vmc_ingress.rs) — `recv_from` → `` `TriggerEvent` ``（`ShutdownBroker` で終了）。`BridgeCatalog` / `BridgeHandles` / `collect_all` / `spawn_all_from_state` に統合。
- **ドキュメント**: [`docs/roadmap/phase-mu-vmc-motion-m0.md`](docs/roadmap/phase-mu-vmc-motion-m0.md) §8、[`docs/roadmap.md`](docs/roadmap.md) Phase M の M-1 tick。例: [`flowgraph.example/vmc-udp-ingress/main.flowgraph.toml`](flowgraph.example/vmc-udp-ingress/main.flowgraph.toml)。`docs/manual/node-catalog.md` 再生成。

### スタイル（stable rustfmt 一括適用）

- `.rustfmt.toml`（stable 互換）に沿い、`src/**` / `build.rs` / `tests/jiff_smoke.rs` を **`cargo fmt` で全面整形**。ロジック変更なし。`cargo test --lib` 全通過で確認。

### Phase M0 — motion 層: VMC 生 UDP パススルー

- **設計**: [`docs/roadmap/v2-vmc-and-restructure.md`](docs/roadmap/v2-vmc-and-restructure.md) を親計画として `docs/roadmap.md` **Active（Phase M）** にリンク。M0 の正本は [`docs/roadmap/phase-mu-vmc-motion-m0.md`](docs/roadmap/phase-mu-vmc-motion-m0.md)。
- **`src/motion/`**: `vmc_raw`（UDP bind + `recv_from` + `ShutdownBroker` 待機）+ `router`（マルチ `send_to`）。`osc` は M4 までプレースホルダ。
- **`[motion]` / `[[motion.vmc_passthrough]]`**: `bind` と `forward_to`（`host:port` 文字列）。パースなしのペイロードコピー転送。例: [`conf.example-motion.toml`](conf.example-motion.toml)。
- **ドキュメント**: `docs/architecture.md`（`motion` 依存方向）、`docs/manual/conf-reference.md` §3.1。

### Phase λ — Flowgraph Enum + ライブラリ再利用（初版）

- **設計**: [`docs/roadmap/phase-lambda-flowgraph-enum-and-library.md`](docs/roadmap/phase-lambda-flowgraph-enum-and-library.md) を正本化。`docs/roadmap.md` で Phase λ を **Completed** に移動し Phase υ / Unscheduled へ cross-link、`phase-delta-spec.md` §8.6 にスキーマ拡張を追記。
- **閉集合 string**: `PortSpec.closed_string_variants`（serde 付き）。`FlowgraphProgram::build` で両端に閉集合があるとき upstream ⊆ downstream を検証。ローダは `flowgraph.literal.string` → 閉集合入力のとき `value` を検証。`flowgraph.tts.speak` の `engine` に TTS レジストリ名を適用。
- **TOML**: `FlowgraphFile.enums`（`[[enums]]`）、`FileMeta` に `author` / `name` / `version` / `license` / `repos` / `library_uses`。`normalized_library_id()` を追加。
- **ライブラリ**: `flowgraph.library.input` / `flowgraph.library.output`（v0 スタブ）を registry に登録。
- **依存**: `load_flowgraph_dir` が `library_uses` の参照存在と閉路を検証（新診断コード）。
- **GUI**: `closed_string_variants` を踏まえた配線判定、`[[enums]]` をパレット `user_defined` に合成、保存時に `[[enums]]` と拡張 `[meta]` をシリアライズ。
- **手動**: [`docs/manual/flowgraph-enum-and-library.md`](docs/manual/flowgraph-enum-and-library.md)。例: [`flowgraph.example/lambda-demo/main.flowgraph.toml`](flowgraph.example/lambda-demo/main.flowgraph.toml)。

### VoicePeak（TTS 実行ファイルパス）

- `conf.toml` に任意の `[voicepeak]`（`path`）を追加。未指定または空のとき、Windows は `%ProgramFiles%\VOICEPEAK\voicepeak.exe`（`ProgramW6432` 優先）、非 Windows はバイナリ名 `voicepeak` をグローバル既定として `State` に保持し、`flowgraph.tts.speak` で `engine="voicepeak"` かつ `endpoint` が空のときに exe として自動補填する。
- VoicePeak ドライバは exe 解決を **`endpoint` を正**とし、`params_schema` の `executable` ヒントを削除。後方互換のため `extra.executable` は `endpoint` が空のときのみ参照し、プロセスあたり 1 回 `warn!` で非推奨を通知する。

### ξ: Dimensional Quantity System (ξ-0 .. ξ-6)

Flowgraph の数値型に **SI 準拠の単位次元システム**を第一級概念として導入。数値に unit を付与、unit は 8 成分次元（Length · Mass · Time · Current · Temperature · Amount · Luminous + 疑似次元 Angle）を持つ。strict default（次元不一致は engine error）+ 明示 escape hatch（`flowgraph.unit.strip`）の方針。既存フローは暗黙 coerce（`Float → dimensionless Quantity`）で完全後方互換。設計詳細: [`docs/roadmap/phase-ksi-dimensional-quantity-system.md`](docs/roadmap/phase-ksi-dimensional-quantity-system.md)、ユーザ向け解説: [`docs/manual/dimensional-quantity-system.md`](docs/manual/dimensional-quantity-system.md)。

- **ξ-0 設計**: phase doc 新設。F# / Haskell の units-of-measure を Flowgraph の pure + 遅延評価エンジンに乗せる方針、SI 準拠 + Angle 疑似次元 + `K` / `ΔK` 分離 + SI 接頭辞 compositional + strict default、を確定。D1-D4 設計決定をユーザ合意のもと文書化。
- **ξ-1 core types** (`src/flowgraph/quantity/*.rs`, 新規モジュール)
  - `Dimension` (8 成分 i8 tuple)、`Unit` (BTreeMap<BaseUnitId, i8> + si_factor + prefix_hint)、`Quantity` (value + unit)、`SIPrefix` enum (Yotta .. Yocto + None)、`BaseUnitId` enum (`Meter` / `Kilogram` / `Second` / `Ampere` / `Kelvin` / `Mole` / `Candela` / `Radian` / `Degree` / `KelvinDelta`) を実装。
  - `parse_unit("m/s^2")` / `parse_unit("kg·m/s^2")` / `parse_unit("μs")` / `parse_unit("MHz")` / `parse_unit("dK")` を自作 parser で処理（外部 crate 依存ゼロ、`uom` crate は compile-time vs runtime の性質不一致で採用見送り）。
  - `Quantity::try_add` / `try_sub` / `try_mul` / `try_div` の演算 API。絶対温度 vs 温度差の semantics は教科書準拠（`K + K` 禁止、`K - K = dK`、`K + dK = K` 等）。
  - unit test 30+ ケース: dimension 代数 / prefix 正規化 / SI base-derived round-trip / parser / convert_to 同次元・異次元エラー / K + ΔK 演算。
- **ξ-2 socket integration + unit nodes** (`src/flowgraph/socket.rs` / `node.rs` / `nodes/unit.rs` (new) / `nodes/collection.rs` / `nodes/state.rs` / `nodes/table_ops.rs` / `registry.rs`)
  - `SocketType::Quantity` / `SocketValue::Quantity(Quantity)` を第一級 variant として追加。`type_of` / `matches` / `as_quantity` 等のアクセサ、`default_value` を整備。
  - `flowgraph.unit.*` PureNode 7 種を実装: `assign` (Float + unit → Quantity) / `convert` (Quantity + target_unit → Quantity、同次元限定、K ⇄ ΔK 拒否) / `strip` (Quantity → Float、明示 escape hatch) / `get_unit_string` / `get_dim_string` / `same_dimension` / `to_json` (`{value, unit, dimension}` の internal form 出口)。全て registry に登録、`default_registry_contains_core_features` に追加。
  - TOML wire format: plain float (dimensionless)、inline table (`{value, unit}`)、quoted string (`"42.5 m/s^2"`) の 3 形式対応。JSON wire format: internal は `{value, unit, dimension}`、外部 IO / pass-through 系（`json_ops` / `state` / `table_ops` / `collection`）は value-only（unit は捨てる、互換性維持）。
  - `Unit::to_si_base` / `from_si_base` を atom-canonical 係数込みで拡張（`Degree` → `Radian × π/180` などの同次元・異 atom 変換を正しく扱えるよう修正）。
- **ξ-3 engine coerce + math migration** (`src/flowgraph/socket.rs` / `engine.rs` / `nodes/math.rs` / `quantity/quantity.rs`)
  - `SocketType::compatible_with(other)` と `coerce_to_type(value, target)` を新設。`Float ↔ Quantity` の暗黙 coerce をエッジ build / runtime 配送の両段で処理。`Float → Quantity` は dimensionless wrap、`Quantity → Float` は dimensionless のみ許容（非 dimensionless は `CoerceError::NotDimensionless` で明示 strip を要求）。
  - `flowgraph.math.float_add` / `float_sub` / `float_mul` / `float_div` を Quantity 演算化。port 型 `Float → Quantity`、内部は `try_add` / `try_sub` / `try_mul` / `try_div`、div-by-zero は `QuantityArithError::DivisionByZero` で明示。次元組み立て（`m * s = m·s`）や次元不一致エラーの unit test を追加。
  - `flowgraph.math.int_*` は「Int は Quantity に乗らない」原則で touched せず。
  - 既存 629 tests 全緑、`BLESS_NODE_CATALOG=1` で catalog 再生成。
  - **(保留)** `SocketType::Quantity { dim: Option<Dimension> }` の struct variant 化は ο-1 以降で compile-time 制約が欲しくなった時点で再評価。ξ-3 時点は unit variant のまま（`dim` 制約は実行時検証）。
- **ξ-4 stringify + format node** (`src/flowgraph/socket.rs` / `nodes/util_format.rs` (new) / `nodes/log.rs` / `nodes/channel.rs` / `registry.rs` / `engine.rs`)
  - `coerce_to_type` / `compatible_with` に `Quantity → String` の一方向暗黙 coerce を追加。`Quantity` の `Display` 実装（`"{value} {unit}"` / dimensionless なら `"{value}"`）を流用。逆方向 `String → Quantity` は非許容（任意文字列の unit parse は不可能）。結果、`flowgraph.util.log` や `flowgraph.channel.emit` の String ポートに Quantity を直接配線できるようになった。
  - `flowgraph.util.format` PureNode 新設。プロパティ: `include_unit: Bool = true` / `precision: Int = -1`（-1 = default Display）/ `unit_override: String = ""`（同次元への事前変換）。精度 / 単位表示 ON/OFF / 表示単位の差し替えを明示制御。
  - `log` / `channel.emit` の node description に stringify ルール（Quantity は自動 `"{value} {unit}"` 化、value-only 欲しければ `strip` か `format(include_unit=false)`）を明記。
  - Integration test `log_receives_quantity_as_formatted_string`: `Float → UnitAssign(m/s^2) → Log` を engine 経由で実行し trace に integrated 表示が載ることを確認。
- **ξ-5 GUI** (`gui/src/lib/flowgraph/FlowgraphNodeCard.svelte` / `FlowgraphCanvas.svelte` / `FlowgraphPropertyEditor.svelte` / `quantityDisplay.ts`): Quantity ポート専用ハンドル色 + 次元 family 別の色、`default` から復元できる場合のみ unit バッジ + 詳細 tooltip。キャンバス接続判定を engine の `compatible_with` に揃え（Float↔Quantity 等）、型不一致エッジを赤破線表示。プロパティ `unit` / `target_unit` / `unit_override` は `GET /flowgraph/parse-unit` で debounce 検証。`GET /flowgraph/node-catalog` の各 Quantity ポート JSON に `quantity_dim` / `quantity_unit_badge` / `quantity_unit_full` を注入。E2E: `flowgraph-canvas-basic` に `math.float_add` の Quantity handle 回帰を追加。
- **ξ-6 ドキュメント** (`docs/manual/dimensional-quantity-system.md` (new) / `docs/manual/index.md` / `docs/roadmap.md` / `CHANGELOG.md` / `docs/manual/node-catalog.md`)
  - ユーザ向け解説ドキュメント `dimensional-quantity-system.md` を新設。動機 / Quantity と Unit の基本 / 使える単位 7 + 10 + 接頭辞 / 単位文字列 parser 文法 / `flowgraph.unit.*` 7 種と `flowgraph.util.format` / 暗黙 coerce ルール / flow TOML リテラル 3 形式 / よくあるパターン 5 件 / FAQ 7 件。
  - `manual/index.md` 目次に追加、Flowgraph 用語の Socket 型列に `quantity` と `table` を追記。
  - `docs/roadmap.md` の Phase ξ ticks を更新、`CHANGELOG.md` に本節を追記。
  - `node-catalog.md` への Dimension 列追加は Phase ο-1 以降（物理計算ノードで次元ラベルが意味を持つ段階）に送る。ξ-6 時点では ξ-2 / ξ-3 / ξ-4 各コミットで blessed 済みの内容で出荷。
- **Breaking（ξ）**: なし。
  - 既存フロー TOML は全て dimensionless `Quantity` として評価され、動作は bit-for-bit 同一。
  - `SocketType::Quantity` variant の追加により、この enum に non-exhaustive match を書いている外部クレート（あれば）は match arm の追加が必要。
  - `flowgraph.math.float_*` 4 種の output 型が `float` → `quantity` に変わったため、GUI 側で上流 / 下流の型マッチを厳密に描画しているコードを持つユーザは再確認推奨（engine の暗黙 coerce により実行は通る）。
- **テスト結果**: `cargo test --lib` 636 passed / 0 failed / 1 ignored（ξ-1 で +30+、ξ-2 で +21、ξ-3 で +4、ξ-4 で +7 の新規 unit / integration test）。

### π: DateTime Type System (π-0 .. π-6)

Flowgraph に **第一級型 `DateTime`（`jiff::Timestamp` ラッパ、UTC 絶対時刻）**を導入し、**Duration は Phase ξ の `Quantity<time>`** に統一（新しい持続時間型は追加しない）。`chrono` crate の既存使用を **jiff へ全面置換**し、直接依存を `Cargo.toml` から解除（間接依存は `twitch-irc` 経由に限定）。IANA タイムゾーン / DST / 暦幅 `Span` は v0 非スコープ。設計: [`docs/roadmap/phase-pi-datetime-system.md`](docs/roadmap/phase-pi-datetime-system.md)、利用者向け: [`docs/manual/datetime-system.md`](docs/manual/datetime-system.md)。

- **π-0 docs**: `phase-pi-datetime-system.md` 新設、roadmap の Phase 名繰り下げ（旧 π OSC 等 → ρ 以降）。
- **π-1 deps + smoke**: `jiff` (serde) 追加、`tests/jiff_smoke.rs` で API 挙動を固定。
- **π-2 chrono → jiff**: 全呼び出し箇所を jiff へ置換（3 batch commit）。serde / TTL / ログ等の wire format は RFC3339 互換範囲を維持（`+00:00` → `Z` 等）。
- **π-3**: `chrono` 直接依存削除。
- **π-4 型基盤**: `src/datetime/mod.rs` に `DateTime` newtype、`parse_with_default_tz`（naive + 固定オフセット）。`SocketType::DateTime` / `SocketValue::DateTime`、String↔DateTime 暗黙 coerce（naive は engine 層では厳格、`parse` ノードで opt-in）。`FlowgraphInstanceConfig` + `conf.toml` の `[flowgraph] default_timezone`（FixedOffset のみ、IANA は拒否）。`parse_offset_str` 独自パーサ。
- **π-5 8 ノード** (`src/flowgraph/nodes/datetime.rs`): `flowgraph.datetime.now` / `.parse` / `.format` / `.add_duration` / `.sub_duration` / `.diff` / `.epoch_ms` / `.from_epoch_ms`。`get_required_datetime` ヘルパ。Phase ο-4 当初案の `flowgraph.time.*` 4 種の役割は本ノード群で代替。lib test +30、`node-catalog.md` 再生成。
- **π-6 docs**: 本 CHANGELOG 節、`docs/manual/datetime-system.md` 新設、`docs/manual/index.md` 目次、[`docs/roadmap/phase-omicron-flowgraph-enhancement.md`](docs/roadmap/phase-omicron-flowgraph-enhancement.md) の §3.4 / §5.4 / §6.4 を π-5 吸収後の記述に更新、`docs/roadmap.md` tick。
- **Breaking（π）**: なし（chrono→jiff は内部表現。JSON 等の RFC3339 文字列は従来どおり解釈可能）。

### ο: Flowgraph Enhancement I（ο-0 .. ο-7、完了）

engine 内完結の **計算・補間・ベクトル・周期タイマー・signal・乱数・ノイズ** ノード群と、**Flowgraph エディタの小さな UX 改善**を 1 フェーズに束ねた。旧案の日時 4 ノードは **Phase π** の `flowgraph.datetime.*` 8 ノードへ移管。外部 OSC/HTTP/プロセス制御等は **ρ / σ / τ / υ / ω** backlog。設計の正本は [`docs/roadmap/phase-omicron-flowgraph-enhancement.md`](docs/roadmap/phase-omicron-flowgraph-enhancement.md)。**Breaking change なし**（新規 crate 依存は `noise = 0.9` のみ）。

- **ο-0 docs**: phase doc 新設、`docs/roadmap.md` の Active 再編、`backlog-nodes.md` §1 の `timer_interval` pointer、angle normalization / 双曲線ノード追記、Phase ξ 依存の明記。
- **ο-1 math（`flowgraph.math.*` 42）**: int/float 別の `abs` / `min` / `max` / `clamp`；float の `lerp` / `inverse_lerp` / `remap` / `smoothstep`；三角・逆三角・`atan2`；双曲線・逆双曲線；`sqrt` / `pow` / `exp` / `log`；`sign` / `floor` / `ceil` / `round`；deg↔rad；`normalize_angle` 系 4。いずれも Quantity-aware。`node-catalog` 各サブフェーズで再生成。
- **ο-2 easing（1）**: `flowgraph.easing.apply` + `curve` enum（linear + quad/cubic/sine/expo/elastic/bounce × in/out/inOut）。`PropertySpec.choices` + GUI dropdown。
- **ο-3 vec（20）**: `flowgraph.vec2.*` / `flowgraph.vec3.*` 各 `make` / `unpack` / `add` / `sub` / `scale` / `dot` / `length` / `normalize` / `lerp` / `distance`（JSON `[x,y]` / `[x,y,z]`）。
- **ο-4 util timer（1）**: `flowgraph.util.timer_interval`（Stateful、`ctx.trigger`、lazy `sources` 例外）。日時ワイヤは π へ。
- **ο-5 signal + random + noise（10）**: Stateful `edge_detect` / `prev_value` / `sample_hold` / `debounce` / `throttle`；Pure `random.uniform_int|uniform_float|normal`；Pure `noise.perlin_1d|perlin_2d`（`Perlin` を seed ごとキャッシュ）。
- **ο-6 GUI**: パレットカテゴリ非表示（`localStorage`）、キャンバス pane への DnD ドロップ、`Ctrl/Cmd+D` 複製、Playwright 回帰（`gui` は E2E 前に `npm run build`）。
- **ο-7 docs 締め**: 本 CHANGELOG 節の総括、`BLESS_NODE_CATALOG=1 cargo test --lib node_catalog_md_up_to_date` で `docs/manual/node-catalog.md` を最終確認、`docs/roadmap.md` で Phase ο を Completed へ移動、phase doc §6.7 完了、`npm run check` / `cargo test --lib` / `npx playwright test` 緑。

### χ: OpenAI Responses API Migration (χ-0 .. χ-8)

Chat Completions (`/v1/chat/completions`) 依存を完全撤去し、OpenAI **Responses API (`/v1/responses`)** を AI ペルソナの唯一の経路に統一した。reasoning model (gpt-5 系) 対応と将来的な hosted tools / encrypted reasoning 採用のための基盤整備。

- **χ-0 設計**: [`docs/roadmap/phase-chi-openai-responses.md`](docs/roadmap/phase-chi-openai-responses.md) にサブフェーズ分割・DTO 設計・移行手順を確定。
- **χ-1〜χ-4: Responses API DTO 層と HTTP クライアント** (`src/ai/openai_responses/`)
  - `CreateResponseRequest` / `Response` / `InputItem` / `OutputItem` / `Tool` / `ToolChoice` / `StreamEvent` 等を自前 DTO で実装（`async-openai` の Chat Completions 型に依存しない）。
  - `ResponsesClient` (`reqwest` ベース) で non-stream / SSE stream の両対応。`OutputTextDelta` / `FunctionCallArgumentsDelta` / `ResponseCompleted` 等のイベントを統一的に処理。
  - tools は Responses API のフラット形式 (`{"type":"function","name":...}`) を native として受け、旧 Chat Completions 形式 (`{"type":"function","function":{...}}`) も後方互換で自動判別。
- **χ-5 AI service 層移行** (`src/ai/service.rs` / `completion.rs` / `context.rs` / `tools.rs` / `model_policy.rs` / `reload.rs`)
  - `AiService` が `ResponsesClient` / `CreateResponseRequest` を保持する構造に一新。`react()` を Responses API の stream + 統合 tool loop (`drive_responses_tool_loop`) に書き換え。
  - gpt-5 系で本文が空になる応答を検出したときの自動リトライ、メモリオーバーフロー要約の Responses API 呼び出しも同経路に統合。
  - 旧 Chat Completions 固有コード（`ChatCompletionRequestMessage` アセンブル、tools → Chat Completions 変換、`extract_assistant_text` 等）は削除。
- **χ-6 設定スキーマ拡張** (`src/ai/config.rs` / `service.rs` / `mod.rs`)
  - `[[ai.personas]]` に Responses API ネイティブな新キーを追加:
    - `openai_max_output_tokens` (u32) — `max_output_tokens` に対応
    - `openai_reasoning_effort` (`"low"`/`"medium"`/`"high"`) — gpt-5 系のみ有効
    - `openai_store` (bool、既定 `false`) — サーバ側会話 state 保存フラグ
    - `memory_overflow_summary_max_output_tokens` (u32)
  - 旧 `max_tokens` (u16) / `memory_overflow_summary_max_completion_tokens` (u16) は新キー未指定時だけ fallback として使われる（両方指定時は新キー優先、warn 無しで legacy を無視）。
  - 環境変数 `VAC_OPENAI_MAX_OUTPUT_TOKENS` による上書きに対応（precedence: env > 新キー > legacy）。
- **χ-7 ドキュメント**: [`conf.example-openai-chat.toml`](conf.example-openai-chat.toml) / [`docs/manual/conf-reference.md`](docs/manual/conf-reference.md) / [`docs/manual/tutorials/openai-persona.md`](docs/manual/tutorials/openai-persona.md) を Responses API 前提に更新。
- **χ-8.0 付随バグ修正** (`src/flowgraph/docs.rs`)
  - `flowgraph::docs::docs_tests::node_catalog_md_up_to_date` が Windows (`core.autocrlf=true`) で CRLF/LF 差分により failing していた件を修正。テスト比較と `BLESS_NODE_CATALOG=1` の書き出しを LF 正規化。
- **χ-8 テスト / 観測器追加** (`src/ai/openai_responses/client.rs` / `src/ai/service.rs`)
  - `cargo test --lib --release` 481 passed / 0 failed / 1 ignored、`npm run check` / `npm run build` (gui) 緑、OpenAI Responses API 3 シナリオ (gpt-4o-mini happy / gpt-4o-mini + tools 宣言 / gpt-5-mini + `reasoning.effort=low`) の実機スモーク成功を確認。
  - `ResponsesClient::create` / `create_stream` に POST 送信・`status` 受信・`Transport` 失敗時の `log::error!` を追加。これまで「`create_stream().await` から戻って来ないのか、SSE 受信待ちなのか」がログから判別不能だったのを解消。
  - `drive_responses_tool_loop` に `StreamEvent::{Created, InProgress, Completed, Incomplete, Failed, Error}` の受信ログを追加（本番は DEBUG、Created/InProgress は TRACE）。SSE ループ終了時に events 数・finalized 有無・failure 要約を 1 行で出す。
  - `reqwest::Client::builder()` に `connect_timeout(10s)` を明示（Windows TLS 交渉ストールで total timeout だけだと事実上無限待ちになるケースを防ぐ）。
- **Breaking（χ）**: なし（既存 `max_tokens` / Chat Completions 形式 tools.json は fallback 経路で互換維持）。ただし **OpenAI 側の最新モデル（gpt-5 系）を使うなら新キーへの移行を強く推奨**。

### ψ-α: Encrypted Reasoning Passthrough (ψ-α-0 .. ψ-α-3)

Phase χ の基盤に乗せる狭い範囲の最適化。gpt-5 系の tool loop で、`store: false` を維持したまま `reasoning.encrypted_content` blob を client 側で持ち回ることで、reasoning token の重複課金と round 間の思考コヒーレンス欠落を緩和する。設計詳細: [`docs/roadmap/phase-psi-alpha-encrypted-reasoning.md`](docs/roadmap/phase-psi-alpha-encrypted-reasoning.md)。

- **ψ-α-0 設計**: χ-11.2 の設計メモを正式な phase doc に昇格。適用範囲を **「1 回の `react()` 内の tool loop round 間のみ」** に限定（複数 `react()` を跨ぐ持ち回りは hot-reload / memory window trim でコヒーレンスが崩れるため対象外）。
- **ψ-α-1 DTO 拡張** (`src/ai/openai_responses/types/`)
  - `CreateResponseRequest.include: Option<Vec<String>>` を追加（Phase ψ-α では `["reasoning.encrypted_content"]` のみ指定）。
  - `OutputItem::Reasoning` に `encrypted_content: Option<String>` を追加（`#[serde(default)]` で既存 response との互換性維持）。
  - `InputItem::Reasoning { id, encrypted_content?, summary? }` variant を新設（前ラウンドの `OutputItem::Reasoning` を次ラウンド input に round-trip するための専用型。wire format は output 側と同じ `type: "reasoning"` + 同フィールド名）。
  - golden tests を 5 ケース追加。特に、同じ JSON を `OutputItem` / `InputItem` どちらにもデシリアライズできることを保証（OpenAI 公式 "pass back reasoning items" の前提）。
- **ψ-α-2 tool loop pass-through** (`src/ai/service.rs` / `src/ai/config.rs`)
  - `openai_reasoning_encrypted_passthrough: Option<bool>` conf key を追加（既定 `true`、gpt-5 系でのみ実効、非 gpt-5 / opt-out 時は完全 no-op）。
  - `apply_reasoning_passthrough_include()` helper で `include` 付与を model 判定と enabled flag に基づいて冪等に実施。
  - `drive_responses_tool_loop` の round 終了時、`collect_reasoning_input_items()` で `OutputItem::Reasoning` を抽出し、`InputItem::Reasoning` として次ラウンド input の FunctionCall / FunctionCallOutput **より先**に積む（OpenAI 公式推奨順）。
  - `encrypted_content` 欠落は warn（API 側応答形状変更の検知用）+ blob 無しで transit（現行動作に degrade）。
  - 単体 tests 9 ケース追加（resolver / include application / Reasoning collection の各パスで no-op / active / dedup / missing blob をカバー）。
- **ψ-α-3 ドキュメント**: 本 CHANGELOG / [`docs/manual/conf-reference.md`](docs/manual/conf-reference.md) / [`conf.example-openai-chat.toml`](conf.example-openai-chat.toml) を更新。
- **Breaking（ψ-α）**: なし。非 gpt-5 モデル（`gpt-4o-mini` 等）の経路は request / input / log すべて bit-for-bit 同一のまま維持される。gpt-5 系ユーザも conf 未指定なら自動 opt-in、デバッグ時は `openai_reasoning_encrypted_passthrough = false` で opt-out 可。
- **計測について**: tool loop が実際に 2 round 以上回るかは `openai_tools_json_path` の tool 構成とユーザ発話依存。VAC 同梱の `vac_ping` / `vac_emit_effect` は 1 round で返るため、実用的な reasoning token 節約の numbers 比較は将来 phase でユーザ作成の multi-round tools で実施する想定。ψ-α-3 時点では（a）DTO 互換性、（b）gpt-4o-mini / gpt-5-mini 双方の既存経路の非退行、を主たる実機検証対象とした。
- **実機スモーク結果（ψ-α-3）**:
  - `conf.chi8-smoke.toml` + OpenAI 実 API で 2 persona を起動し ingress を投入。
  - **gpt-4o-mini 経路**: `ψ-α: include=...` 付与ログは**出ない**（no-op を確認）、POST body=285B、`SSE ループ終了 (events=16, finalized=true)` — ψ-α 前と bit-for-bit 同一挙動。
  - **gpt-5-mini 経路**: `ψ-α: include=reasoning.encrypted_content を付与（model=Some("gpt-5-mini"), passthrough=true）` が appears、POST body=392B（include 分 +107B）、OpenAI 側 `status=200 OK`、`SSE ループ終了 (events=19, finalized=true)` — include キーは API に受理され、既存 stream / content 流路も正常完了。
  - cargo test --lib は ψ-α 関連 unit tests（DTO golden + resolver/helper）をすべて緑で通過し、既存テストの退行なし。

### ν: GUI E2E Testing with Playwright (ν-0 .. ν-2)

GUI の自動回帰テストをゼロから構築。Playwright を `gui/` 配下に閉じ込めて導入し、外部 IO なしの fixture (`conf.fixture.e2e.toml`) を `webServer` で `cargo run --release` 起動して、本番と同じ `gui/dist` 経路で叩く。初期 5 ケースのうち **3 ケース**（`control-panel-smoke` / `channels-ws-live-update` / `live-quick-add-learn-undo`）を着地させ、残る 2 ケース（flowgraph canvas DnD / dictionary editor 409 merge）は **Phase ν-β** として分離した。設計詳細: [`docs/roadmap/phase-nu-gui-e2e-playwright.md`](docs/roadmap/phase-nu-gui-e2e-playwright.md)。

- **ν-0 設計**: phase doc 新設。fixture 設計 / 5 ケース仕様 / webServer 戦略 / セレクタ規約（`data-testid` より accessible name を優先）を固定。
- **ν-1 Playwright 基盤** (`gui/package.json` / `gui/playwright.config.ts` / `gui/tests/e2e/` / `gui/.gitignore` / `gui/eslint.config.js` / `conf.fixture.e2e.toml` / `gui/tests/e2e/fixtures/`)
  - `@playwright/test` を devDependency として導入。`test:e2e` / `test:e2e:ui` / `test:e2e:install` の npm script を追加。
  - `playwright.config.ts` で workspace root を `cwd` として `cargo run --quiet --release --bin virtual-avatar-connect -- conf.fixture.e2e.toml` を起動する `webServer` を設定。`baseURL='http://127.0.0.1:57098'`、`workers: 1`。
  - `conf.fixture.e2e.toml`: `workers=2` / `log_level="Debug"` / `web_ui_address="127.0.0.1:57098"` / `runtime_dir="target/e2e-runtime"` / `flowgraph_dir="gui/tests/e2e/fixtures/flowgraph"` / `[control_api] bearer_token="e2e-fixture-token"` + `require_token_for_*=true`。Twitch / OpenAI / TTS / OCR / Voice / Translate は全セクション未設定で外部 IO ゼロ。
  - 共有ヘルパ (`gui/tests/e2e/fixtures.ts`): `TOKEN` 定数 + `authHeader()` + `tokenQuery()`。
  - fixture flowgraph (`gui/tests/e2e/fixtures/flowgraph/sample.flowgraph.toml`): `web_input → log` の §3.5 用経路 + `dictionary.learn` / `dictionary.forget` + `table.from_json` の §3.4 用 Pure チェーン。
- **ν-2 初期 spec（3 ケース）** (`gui/tests/e2e/*.spec.ts`)
  - **§3.1 `control-panel-smoke`**: `/gui/?token=...` ロード → `<nav aria-label="Main tabs">` 配下の "Live" ボタン可視 → `GET /api/v1/control/ping` が 200 を返す。`TabNav.svelte` が `<button>` ベースで `role="tab"` を使わない構造に対応。
  - **§3.5 `channels-ws-live-update`**: `page.waitForEvent('websocket')` で `/api/v1/control/events` への接続を掴み、`POST /api/v1/control/ingress` で投入したユニーク content が `channel_datum` フレームとして同じ session に配信されることを `framereceived` で検証。`ControlEvent::ChannelDatum` が flat struct (`channel`/`content` が top-level) である点に合わせて predicate を確定。
  - **§3.4 `live-quick-add-learn-undo`**: `DictionaryLiveQuickAdd` 経由で `POST /flowgraph/default/trigger/sample::learn` を `waitForRequest` 捕捉（body に `source`/`replacement`/`kind=literal`/`by=gui:quick_add`）→ 履歴カウンタが (1) へ → [履歴] 展開して [Undo] で `sample::forget` が `mode=latest` で飛ぶ → 行 label が "Undone" に遷移、までの UX を検証。trigger は 202 Accepted を期待。
  - 実行時間: 3 specs / 1 worker で **8.4s**（chromium headless, cold `cargo run --release` は `reuseExistingServer` で回避）。
- **ν-β に分離したもの**（**ν-β で着地済み**、下の節を参照）
  - §3.2 `flowgraph-canvas-basic`（Svelte Flow の DnD 自動化は座標ベース合成イベントが鬼門で ν のスコープを外れる）
  - §3.3 `dictionary-editor-409-merge`（editor state machine + 別クライアントで revision を進める race condition + conflict dialog の UI セレクタが §3.4 より UI 依存度が高い）
  - ν-2.4 実装中に engine 側の `PortSpec::with_default(SocketValue::Table(Table::empty()))` が `MissingRequiredInput` を吐く現象を回避するため fixture に `flowgraph.table.from_json` を挟んでいる。ν-β で engine 側を直し、fixture の補助ノードを除去する計画。
- **Breaking（ν）**: なし。既存 conf / runtime / GUI 経路に一切手を入れていない（`gui/` 配下の新規ファイルと `conf.fixture.e2e.toml` / `gui/tests/e2e/fixtures/` の追加のみ）。

### ν-β: Flowgraph Canvas + Dictionary Editor E2E (ν-β-1 .. ν-β-3)

ν-2 から分離していた残 2 spec と、道連れで見つかっていた engine 側バグをまとめて着地。最終的に `npx playwright test` は **5 specs / 1 worker / ~9 s** で全緑。詳細: [`docs/roadmap/phase-nu-gui-e2e-playwright.md`](docs/roadmap/phase-nu-gui-e2e-playwright.md) §3.2 / §3.3 / §6.4。

- **ν-β-3 fix(flowgraph/table)** (`src/flowgraph/table.rs` / `src/flowgraph/node.rs` / `gui/tests/e2e/fixtures/flowgraph/sample.flowgraph.toml`)
  - `Table::from_json_array(&[], None)` が「空配列 + スキーマ未指定」でスキーマ推論に失敗していた挙動を修正。空入力時は早期に `Table::empty()` を返すようにし、`PortSpec::with_default(SocketValue::Table(Table::empty()))` → JSON `[]` → `to_socket_value` の round-trip を成立させた。これにより `dictionary` 入力 (Table) の default が coerce 経路で `MissingRequiredInput` にならなくなる。
  - fixture flowgraph から補助ノード `dict_src` (`flowgraph.table.from_json`) と関連 edge を削除し、`in / log / learn / forget` の 4 ノード最小構成に戻した（コメントで「default coerce が engine 側で通るため補助ノードは不要」と明示）。
  - ユニットテスト 3 本追加: `from_empty_json_without_schema_returns_empty_table` / `empty_table_json_roundtrip_is_idempotent` / `port_default_table_empty_round_trip`（port default → `to_socket_value` の型一致確認）。
- **ν-β-1 test(gui): §3.3 `dictionary-editor-409-merge.spec.ts`** (`gui/tests/e2e/dictionary-editor-409-merge.spec.ts` / `.gitattributes`)
  - 実 409 `optimistic_lock_failed` を踏ませるシナリオ: GUI が「更新する」を押す直前に、Playwright `request` で同じ row を `If-Match: b3:<hash>` 付きで PATCH → GUI 側 hash が stale 化 → 409 → `DictionaryConflictDialog` 起動 → 3-way 表で mine / server 両方の replacement と `replacement` カラム名の並びを確認 → 「自分の編集を強制」で fresh hash で 200 書き戻し。最終的に API 直叩きで mine 値に確定していることを確認。
  - Preflight / cleanup で `Dr.USAGI` 行の replacement を `ドクターウサギ` に戻し、`sample.dict.tsv` に差分を残さない（自己治癒 + 繰り返し実行に耐える）。
  - `.gitattributes` 新設: `gui/tests/e2e/fixtures/**/*.tsv` を `text eol=lf` で固定し、Windows の `core.autocrlf=true` 環境で VAC が LF で書き戻した TSV が CRLF 差分として `git status` に出続けるノイズを潰した。
- **ν-β-2 test(gui): §3.2 `flowgraph-canvas-basic.spec.ts`** (`gui/tests/e2e/flowgraph-canvas-basic.spec.ts` / `.gitignore`)
  - Flowgraph タブ → `sample` を開き、Palette の検索に `util.log` を入れて `flowgraph.util.log` を 1 件に絞ってからクリック → `log_2` を追加 → Save ボタンが `Save` → `Save *` に遷移 → `Ctrl+S` → `PUT /api/v1/control/flowgraph/file/sample` 200 → label が `Save` に戻る、までの往復を `waitForResponse` で検証。PUT body に `id = "log_2"` と `flowgraph.util.log` が含まれることも assert。
  - Svelte Flow の **handle drag による edge 接続** と **`beforeunload` ガードの発火** は pointer event 合成と Chromium の dialog 仕様が絡んで flake 要因が強いため、spec 冒頭にコメントで意図を明記した上でスコープ外に置いた（ν+ へ送る）。
  - `try / finally` で preflight snapshot の TOML を `PUT` し直し、fixture を汚さない。
  - VAC の write 系 API が毎回生成する `*.bak-YYYYMMDD-HHmmSS` バックアップをリポジトリ外に落とすため `.gitignore` に `**/*.bak-[0-9]*` を追加（flowgraph / conf / profiles 全経路で共有の命名なので広めにマッチ）。
- **Breaking（ν-β）**: なし。engine 側の fix は従来 error だった経路を success にするだけの緩和、GUI / conf / wire format は一切変更なし。

### η: Dictionary/Table Unification (η-0 .. η-6)

V1 時代に合意されていた「11 カラム辞書」仕様（`source` / `replacement` / `kind` / `priority` / `is_locked` / `enabled` / `by` / `created_at` / `expires_at` / `tags` / `note`）と runtime 学習/忘却機能を V2 Flowgraph 上に再構築した。汎用 Table 型を型システムに追加し、辞書機能はその上に semantic layer として乗る。

- **`SocketType::Table` 追加** (η-1a)
  - Flowgraph 型システムに 9 番目の variant として導入。`List<Json>` とは別の第一級型として `Table` を扱う（暗黙変換なし、`flowgraph.table.*` 経由で明示）。
  - `SocketValue::Table(Table)` で値を保持。`Table` は `Arc<TableInner>` + lazy `content_hash: OnceLock<[u8; 32]>` + `version: u64` でできた clone O(1) / COW mutation 構造。
- **`src/flowgraph/table.rs` 新設** (η-1b)
  - `Table` / `TableSchema` / `ColumnSpec` / `Row` の構造体群。`make_mut` / `push_row` / `remove_last_where` / `remove_all_where` で COW-based mutation、`content_hash()` は blake3 の lazy 計算。
  - `Table::to_json_array()` / `Table::from_json_array()` で serde_json 側との相互変換を提供。
- **`flowgraph.table.*` 4 ノード追加** (η-2)
  - `flowgraph.table.from_json` (Pure): `List<Json>` → `Table`（スキーマは先頭 object から推論）。
  - `flowgraph.table.to_json` (Pure): `Table` → `List<Json>`。
  - `flowgraph.table.load_tsv` (Effectful): TSV ファイル → `Table`。モード `auto` / `headerful` / `legacy_loose` を支持し、V1 の space/TAB 2 列フォーマットを `legacy_loose` で吸収。エスケープは `\\` / `\t` / `\n` / `\r`。
  - `flowgraph.table.write_tsv` (Effectful): `Table` → TSV ファイル。`path.tmp` への書き出し後 atomic rename。
- **`flowgraph.dictionary.*` 4 ノード刷新** (η-3)
  - `flowgraph.dictionary.replace` (Stateful、刷新): 入力を `Dictionary: List<Json>` から `Dictionary: Table` に変更。内部で literal エントリは Aho-Corasick（`LeftmostLongest`）で一括置換、regex エントリは priority desc 順に `Regex::replace_all`。`Arc identity → version → blake3 content_hash` の 3 段キャッシュで AC/Regex 再コンパイルを回避。`enabled=false` / `expires_at` 期限切れは compile 時に除外。
  - `flowgraph.dictionary.match` (Stateful、新規): `text` と `dictionary` から照合結果を出す。`match_policy = first / all / longest`、`anchor = anywhere / prefix / full`。出力 `matched_entries: List<Json>` / `matched_count: Int` / `captures: List<List<String>>` / `first_replacement: String`、exec は `on_match` / `on_no_match`。
  - `flowgraph.dictionary.learn` (Pure、新規): Table に 11 カラムエントリを append。同一 `(source, replacement, kind)` & 有効エントリが既存なら `on_duplicate`、新規なら `on_learned`。`created_at` は RFC3339 UTC 秒精度で自動採番。`is_locked` は常に `false` で追加。
  - `flowgraph.dictionary.forget` (Pure、新規): `mode = latest / all / exact` で削除。`is_locked=true` は除外してロックカウントを報告。`latest` は「最新 1 件を消すことで過去エントリが自動復活する UNDO 動作」を保持。
- **旧 `flowgraph.dictionary.command` ノード削除** (η-4)
  - V1 由来の固定文法（`学習(X:=Y)` / `忘却(X)` 等）をノード内部に埋め込んでいたが、`flowgraph.dictionary.match` + `flowgraph.dictionary.learn` / `.forget` + ユーザー編集可能な `commands.tsv` で表現する方式に置き換え。コマンド構文を変更・拡張する場合はノード実装ではなく TSV を書き換えればよくなった。
- **依存追加**
  - `blake3 = "1.5"` — Table content_hash（Stateful Replace/Match キャッシュ識別）。
  - `aho-corasick = "1.1"` — literal 辞書マッチング高速化。
- **サンプル flowgraph 追加**
  - `flowgraph.example/dictionary/basic-replace.flowgraph.toml` — TSV ロード → 辞書置換 → ログ出力の最小例。
  - `flowgraph.example/dictionary/command-dispatch.flowgraph.toml` — `commands.tsv` を `flowgraph.dictionary.match` に食わせて「学習 / 忘却」コマンドを判別する例。
  - `flowgraph.example/dictionary/commands.tsv` — 学習 / 忘却コマンド syntax のサンプル。
  - `flowgraph.example/dictionary/sample.dict.tsv` — 固定辞書 + 通常エントリ + regex のサンプル。
- **GUI: Table ポート視覚区別** (η-6)
  - `gui/src/lib/flowgraph/FlowgraphNodeCard.svelte` の `handleClass` を更新し、`port.ty === 'table'` の Handle に `flowgraph-handle data table` クラスを付与。emerald-500 の角丸正方形（12×12px）で、通常の青円データポート / 橙三角 exec ポートと明確に形状区別。
  - `flowgraph.dictionary.*` / `flowgraph.table.*` ノードはそれぞれ `category = "dictionary"` / `"table"` を返すため、既存 `FlowgraphPalette` の自動グルーピングで独立セクションとして出る（追加実装不要）。
  - **保留**: 11 カラム編集 UI (Dictionary Editor pane) と Live quick-add widget は V2 に Table ファイル直接編集用の Control API が未整備のため φ フェーズ以降で実装予定。仕様書 §9.2 / §9.3 に保留理由を明記。
- **V1 → η migrate CLI** (η-5)
  - `virtual-avatar-connect-migrate-dict` 独立 bin を追加（`src/bin/migrate_dict.rs`）。`clap` ベース、`--input path[:kind[:tag]]` を複数指定可能で複数ファイルを 1 本の 11 カラム TSV に merge。
  - V1 `dictionary.*.txt`（loose 空白 2 列）、`regex.*.txt`（replacement + pattern、multi-word replacement は "空白 + `^`" で境界推定）、`regex.*.csv`（CSV 標準クォート付き `replacement,pattern`）をサポート。
  - `--locked` 指定でファイル由来エントリを `is_locked=true` に固定、`--by` で登録者 ID を付与。出力は atomic rename、既存ファイルは `--force` 指定がないと上書きしない。
  - 既存の `dictionary.arknights.txt` / `dictionary.pre-coeiroink.txt` / `dictionary.chat.txt` + `regex.chat.csv` / `dictionary.local.txt` / `regex.pre-command.txt` / `regex.local.txt` を実際に η TSV に変換済み（該当 `.dict.tsv` を生成）。
- **ドキュメント**
  - `docs/roadmap/phase-eta-dictionary-unification.md` — 11 章構成の仕様書（Background / Goals / Data Model / File Formats / Type System Extension / Node Catalog / Caching & Performance / Migration / GUI / Test Plan / Open Questions）。§8.1 に migrate CLI の実装済み仕様を反映。
  - `docs/roadmap/phase-delta-spec.md` の §2.1 に `Table` variant を追記、§10.2（TSV 化 ε 枠）を η で完了済みに更新、feature 命名規則の例に η 新ノードを列挙。

### Breaking changes (η)

- `SocketType` enum に `Table` variant が追加されたため、この enum に対して non-exhaustive match を書いている外部クレート/スクリプト（あれば）は追加対応が必要。
- `flowgraph.dictionary.replace` の `dictionary` 入力ポートの型が `List<Json>` から `Table` に変更。既存の flowgraph ファイルは `flowgraph.table.from_json` を挟むか、TSV を読み込ませるように再配線する必要がある（同 feature 名だが型が非互換）。
- `flowgraph.dictionary.command` ノードは削除された。同等機能は `flowgraph.dictionary.match` + `flowgraph.dictionary.learn` / `.forget` + `commands.tsv` で組み直す。

### v2 merge-ready roadmap (ζ-3 / γ-4a.0 / γ-4a / γ-2 / δ-X / ζ-4)

- **ζ-3: Flowgraph reload 時のブリッジ再配線**
  - `BridgeHandles` 構造体を新設し `SharedState.bridge_handles` に常時保持。起動時 spawn と reload 時 respawn を同じ経路に統一。
  - `web_interface::control::flowgraph::reload_runtime` が旧 `FlowgraphRuntime` worker と旧 bridges の両方を graceful shutdown してから新しい一式を spawn。
  - `web_input` endpoint 差分時は `ControlEvent::RestartRecommended` を push して GUI にリスタート推奨トーストを出す（actix route は hot-swap 不可）。
- **γ-4a.0: GUI ノード/エッジ削除 UX**
  - `FlowgraphNodeCard` に hover 時の × ボタンを追加、`FlowgraphCanvas` の `ondelete` を multi-select 削除に接続。
  - `flowgraphStore.removeSelection` / `undoLastDelete` を追加し、削除直後の toast に「元に戻す」action ボタンを表示。
- **γ-4a: Pipeline エディタ基盤**
  - `flowgraphStore.isDirty` getter を追加し、`FlowgraphTab` の Save ボタンを `Save *`（warning 色）で dirty 表示。
  - Ctrl+S / Cmd+S で保存する window 級キーボードショートカットと、dirty 状態のとき `beforeunload` で確認ダイアログを出す移行ガード。
- **γ-2: Live タブ仕上げ**
  - Managed App の `POST /api/v1/control/managed_apps/{id}/restart` を追加（stop → start の合成、`supports_status=false` は非対応で 400）。
  - `ManagedAppDrawer` に再起動ボタン、`BosPreview` に縦画面時アコーディオン（手動トグル可）を追加。
- **δ-X: `flowgraph.command.set` ノード**
  - V1 `feature = "command"` の scene switcher 相当を Flowgraph ネイティブに復活。プロパティ `sets` に `{name, pre, post, channel_contents}` を配列で持たせ、入力 `command_name` に一致した set を `pre → channel_contents → post` 順に emit。
  - サンプル `flowgraph.example/command-sets/main.flowgraph.toml` を追加。
- **ζ-4: AI-Twitch ハイブリッド方針の明文化**
  - `conf.example-openai-chat.toml` の `custom_instructions` セクションに `vac_twitch_chat_say` の利用基準を行動規範サンプルとして追記。
  - 既存の `vac_twitch_chat_say` ツールスキーマ（`conf.example-openai-tools.json`）と `flowgraph.example/twitch-events/`（C1 標準 flowgraph 群）とで、定型＝Flowgraph / 判断＝AI tool の分業を明示。

### Breaking changes

- **V1 processor 層と V1 Control API エンドポイントの除去** (δ-9 D.2/D.3/D.4/D.5)
  - `src/processor/` 配下の V1 処理系ファイル (`aivis_speech.rs`, `bouyomichan.rs`, `coeiroink.rs`, `command.rs`, `dictionary_command.rs`, `gas_translation.rs`, `libre_translation.rs`, `modify.rs`, `os_tts.rs`, `voicevox.rs`, `twitch_out.rs`) を一括削除。
  - `Processor` trait / `ProcessorKind` enum / `ProcessorRuntime` 構造体 / `init_processors` / `dispatch_processors_for_incoming` / `conf.v1_processors_enabled` を削除。Flowgraph Runtime が唯一の実行エンジン。
  - `State` から `processors: Vec<ProcessorKind>` / `processor_runtimes: Vec<ProcessorRuntime>` / `v1_processors_enabled: bool` フィールドを除去。
  - Control API の V1 エンドポイントを除去:
    - `GET/PUT /api/v1/control/processors/{index}/config`
    - `GET/PUT /api/v1/control/ai_personas/{index}/config`
    - `POST /api/v1/control/modify/content/...` 系 (dictionary / regex 編集 API)
    - `POST /api/v1/control/modify/open` (エディタ連携)
    - `POST /api/v1/control/reload` の `target: modify_files`
    - `PauseTarget::Processors` / `PauseTarget::Processor { .. }` (pause/resume は AI persona 限定に)
  - `StateSnapshot` DTO から `processors: Vec<ProcessorSummary>` を除去。`schema` を 1 → 2 に bump。
  - `CoeiroInk` / `AivisSpeech` / `VOICEVOX` の diagnostic section / CLI (`--coeiroink-speakers` / `--aivisspeech-speakers` / `--voicevox-speakers` / `--test-os-tts`) は v0.9.x では一時停止。Flowgraph TTS ドライバ経由で δ-9.3 に再実装予定。
  - GUI: `Pipeline` タブおよび関連 Svelte コンポーネント (`PipelinePanel.svelte` / `ProcessorNode.svelte` / `IngressNode.svelte` / `NodePropertiesPanel.svelte` / `DictionaryEditorModal.svelte` / `RegexEditorModal.svelte` / `AiNode.svelte` / `pipelinePulse.svelte.ts`) を削除。タブナビゲーションから `pipeline` を除去。`#pipeline` URL は `flowgraph` へフォールバック。
  - `[[processors]]` は TOML 層には残るが State では使われない (パース時に警告を出す)。δ-9.3 で migrate CLI の最終形を整えたあと、次期バージョンで conf スキーマからも除去予定。

### 既知の回帰

- `windows` crate の 0.61 → 0.62 アップデートに伴い、`src/processor/ocr/mod.rs::recognize` と `src/processor/screenshot/windows.rs` の `Error::from_win32()` / `IAsyncOperation::get()` API が失効。暫定で `recognize` は常に `anyhow::bail!` を返し、GDI 側は `E_FAIL` ベースのエラーに fallback。δ-9.3 で `windows-future` 経由の実装に差し替え予定。

## [0.9.0] - δ-9 Part A–C: Flowgraph Runtime 並走稼働

本リリースは **Flowgraph Runtime を初めて V1 processor 層と並走させた** マイルストーン。V1 除去 (当初 δ-9 Part D-F の予定) は scope の大きさから **δ-9.2 へ分割延期** した。既存の `[[processors]]` 設定は引き続き動作する。

### Breaking changes

なし。既存 V1 設定 (`conf.toml` / `conf.local.*.toml`) はそのまま動作する。

### 追加

- **Flowgraph Runtime の常駐稼働** (Part A)
  - `conf.flowgraph_dir` が指すディレクトリにフローグラフを置くと、起動時に専用 tokio worker に `FlowgraphProgram` が move 所有され `run_forever_with_bus` が走り続ける。
  - `ExecCtx.state_handle: Option<Weak<RwLock<State>>>` を追加し、Effectful ノードから `State::push_channel_datum` 等を呼べるようにした。
  - 終了時は `ctrl-c` / `RuntimeHandle::shutdown` で worker が grace に停止する (broadcast 経由)。
- **`flowgraph.ingress.*` ノードの property 化** (Part B)
  - `flowgraph.ingress.web_input`: `path`, `method`, `body_format`, `fixed_channel`
  - `flowgraph.ingress.voice`: `engine`, `model_path`, `source`, `sample_rate`, `language_code`, `grammar_json`, `fixed_channel`
  - `flowgraph.ingress.twitch`: `mode`, `channels`, `access_token`, `client_id`, `broadcaster_id`, `login`, `fixed_channel`
  - `LoadedNodeMeta` に解決済み `properties: InputMap` を追加し、bridge 層からロード時 property を参照可能に。
- **`src/bridges/` 外部ブリッジモジュール** (Part B)
  - `bridges::web_input`: `flowgraph.ingress.web_input` ノード群を actix-web の `ServiceConfig` に自動登録する完全実装。plain / json / form の body decode + `TriggerEvent` 生成 + `TriggerHandle` 送信。
  - `bridges::voice`, `bridges::twitch`: property 抽出のみ (実ワーカーは V1 `src/processor/voice_*` / `twitch*` 共用、δ-9.2 で差替え)。起動時に暫定注意ログを出力。
  - `bridges::collect_all` で `FlowgraphRuntime::node_meta` から ingress エントリを一括抽出。
- **`flowgraph.channel.emit` ノード** (Part C)
  - inputs: `exec_in`, `channel`, `content`, `is_final`, `source_actor`
  - outputs: `exec_out` (成功) / `on_error` (state_handle 不在 / channel 空 / state drop 済み)
  - `ExecCtx.state_handle` 経由で `State::push_channel_datum` を呼び出し、Flowgraph 終端を V1 の ws / browser-output 配信経路に接続する。

### 変更

- `FlowgraphRuntime` は diagnostic-only から「diagnostics + ノードメタ + 実行ハンドル (`RuntimeHandle`)」保持型へ拡張。`load_and_spawn` でロード＋worker spawn を一括実行する。
- `LoadReport` の消費パスが `load_program` / `load_and_spawn` の 2 経路に分岐。旧来の `FlowgraphRuntime::load` は program を drop する互換シムとして残っている (テスト・GUI 単独起動用)。

### 延期（δ-9.2 以降）

- V1 `[[processors]]` / `Processor` trait / `dispatch_processors_for_incoming` の除去
- `src/conf/processor_conf.rs` / Control API V1 エンドポイント / GUI Pipeline タブの除去
- `src/bridges/voice.rs` / `twitch.rs` の完全実装
- `conf.local.*.toml` からの `[[processors]]` 除去と `flowgraph.local/` 完全移行
- `Cargo.toml` `1.0.0` major bump と Breaking Changes 一斉告知

### テスト

- 全 360 ユニットテスト pass（flowgraph 219 + tts 各 driver 含む、web_input ブリッジ 4 / voice 2 / twitch 2 / channel.emit 3 新規）。
- `BLESS_NODE_CATALOG=1` で `docs/manual/node-catalog.md` を再生成済み（`flowgraph.channel.emit` を追加）。

---

以前の Phase は `docs/roadmap/phase-*` を参照。
