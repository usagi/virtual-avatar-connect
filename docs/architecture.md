# VAC Architecture

Virtual Avatar Connect のレイヤ構成と依存方向、および開発時の Commit Granularity Rule。
詳細な Flowgraph 仕様は [`roadmap/phase-delta-spec.md`](roadmap/phase-delta-spec.md)、フェーズ進行は [`roadmap.md`](roadmap.md) を参照。Enum / ライブラリ再利用の設計正本は [`roadmap/phase-lambda-flowgraph-enum-and-library.md`](roadmap/phase-lambda-flowgraph-enum-and-library.md)（Phase λ）。

---

## Scope

- **Windows / Linux / macOS 単独アプリ**（現状は `cargo run -- <conf>.toml` の **単一バイナリ**）
- **Flowgraph-only アーキテクチャ**（v0.10.0〜）: ingress〜変換〜出口は `flowgraph_dir` 配下の `.flowgraph.toml` と Flowgraph Runtime で表現
- **内蔵 HTTP サーバ**: `actix-web` で GUI 配信 + Control API + WebSocket
- **GUI**: Svelte 5 + Vite、`gui/` 配下の独立プロジェクト（build 成果物は `gui/dist/`）。**リリース**では `gui/dist` をビルド時にバイナリへ取り込み、`npm run dev` なしで設定 GUI を扱う方針（[`roadmap/v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md) §1.2）

---

## Layering

```text
                        ┌──────────────────────┐
                        │  gui/  (Svelte + TS) │
                        └──────────┬───────────┘
                                   │ HTTP/WS (Control API)
 ingress → flowgraph → egress      ▼
 ┌───────┐   ┌────────┐   ┌─────────────────┐
 │bridges│──▶│flowgraph│──▶│managed_app/     │
 │twitch │   │ runtime │   │libretranslate/  │
 │voice  │   │         │   │ai (OpenAI)      │
 └───────┘   └────┬────┘   └─────────────────┘
                  │
                  ▼
          ┌───────────────┐
          │ state (shared)│
          └───────────────┘
```

データは `ChannelDatum` を軸に `SharedState` / `SharedChannelData` を経由して各層に配信される。Flowgraph は `SocketType` × `SocketValue` のタイプ付きエッジで内部配線する。

---

## Module Organization

### `src/lib.rs` / `src/main.rs` / `src/runtime.rs`

- プロセスエントリ、`run()` の wiring
- `ShutdownBroker` 初期化、Ctrl+C listener、cleanup フェーズ

### `src/conf/`

- `conf.toml` の deserialize（`ConfLayout` / `ControlApiConf` / `AiConf` / `TwitchConf` / etc.）
- 構造体階層は TOML スキーマと 1-to-1

### `src/state/`

- `SharedState` / `SharedChannelData` / `ChannelDatum`
- broadcast channel、hot-reload ハンドル、`AiRuntime` / `FlowgraphRuntime` 等の runtime 登録

### `src/flowgraph/`

- Flowgraph Runtime 本体
- `node.rs` (NodeSpec, NodeDescriptor)、`registry.rs`、`table.rs`、`socket.rs`
- `nodes/` にビルトインノード群（`dictionary/` / `table_ops/` / `twitch/` / `tts/` / `util/` ...）
- loader / docs / state 維持系サブモジュール

### `src/ai/`

- OpenAI ベースの AI Persona サービス（1 ペルソナ = 1 常駐タスク）
- `service.rs` event loop、`completion.rs` LLM 呼び出し、`context.rs` メッセージ組み立て、`tools.rs` function calling、`model_policy.rs` モデル別戦略、`reload.rs` hot-reload、`decision.rs` 発話判定、`observe.rs` トリガ条件
- `fine_tuning/` は Files / Fine-tuning API の管理（Chat Completions / Responses と直交）
- **Phase χ 以降**: `openai_responses/` サブモジュールに自前 `reqwest` ベース Responses API クライアントを追加予定（shared crate 化を視野に `crate::*` 非依存）

### `src/web_interface/`

- `actix-web` サーバ
- `control/` 配下に Control API（`/api/v1/control/*`）: file CRUD / flowgraph CRUD / table CRUD / flowgraph trigger / shutdown / pause / resume
- WebSocket（`/ws/control`）

### `src/bridges/`

- Flowgraph ⇄ ingress/egress のブリッジ配線（web_input / voice / twitch / twitch_eventsub / channel_subscribe / **vmc_ingress**（Phase M1））
- reload 時に `BridgeHandles` を `SharedState` に保持して graceful 再配線

### `src/motion/`（Phase M0〜）

- VMC 互換の **生 UDP パススルー**（`[motion]`、`ShutdownBroker` 連携）。パースは後段（M4 / Phase ρ 系）
- 設計: [`roadmap/v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md)、M0 正本: [`roadmap/phase-mu-vmc-motion-m0.md`](roadmap/phase-mu-vmc-motion-m0.md)

### `src/twitch/`

- EventSub / Helix / IRC
- トークン管理、user resolution

### `src/managed_app/`

- 子プロセス管理（CoeiroInk / VOICEVOX / LibreTranslate など）
- `stop_all_graceful` で WM_CLOSE → grace → TerminateProcess

### `src/libretranslate/`

- LibreTranslate HTTP クライアント + Managed App 連携

### `src/shutdown.rs`

- `ShutdownBroker`（Ctrl+C / Control API / Fatal / Tauri の 4 経路を単一 broker に集約）

### `src/migrate/`

- `virtual-avatar-connect-migrate-dict` CLI（V1 辞書 → η 11 カラム TSV）

### `gui/`

- Svelte 5 + Vite + TypeScript
- `src/lib/flowgraph/` Flowgraph エディタ、`src/lib/control/` Control API クライアント、`src/lib/tabs/` タブ UI
- build 成果物 (`gui/dist/`) が `actix-web` から配信される

---

## Dependency Direction

- `lib.rs` / `main.rs` は全 feature モジュールに依存
- `conf` / `state` / `shutdown` は core utility 相当、他モジュールから参照されるが自身は最小依存
- `flowgraph` は `state` / `conf` / `shutdown` に依存、ingress 系（`twitch` / `bridges`）とは broadcast 経由で疎結合
- `motion` は `conf` / `shutdown` のみに依存（Flowgraph 非依存）。将来 `vmc_ingress` ブリッジから利用予定（Phase M1）
- `ai` は `state` / `conf` / `shutdown` に依存、`flowgraph` とは独立（Flowgraph ノードとしての embedding は将来拡張）
- `web_interface` は全モジュールに依存（Control API が runtime 状態を触るため）
- `gui` は HTTP/WS 経由でのみ `web_interface` に依存、Rust コードへの直接依存なし

**Phase χ 以降の内部細分**:

- `ai/openai_responses/` サブモジュールは `crate::*` に依存しない（`SharedState` / `ChannelDatum` 非参照、input/output は自己完結型）。将来 `vac-openai-responses` crate への切り出しを可能にする

---

## Commit Granularity Rule

UNVET (`usagi/un-virtual-eye-tracker`) の convention を踏襲し、Phase χ 以降に本格適用する。

1. **Phase が major work unit**（`δ` / `ε` / `ζ` / `η` / `φ` / `χ` / ...）
2. **サブフェーズが commit unit**（`χ-0`, `χ-1`, ...）
3. **1 commit = 1 behavior topic**（例: SSE パーサと DTO 定義を同じ commit に混ぜない）
4. **Commit prefix**: `<sub-phase> <type>(<scope>): <summary>`
   - 例: `χ-1 feat(ai/responses): scaffold openai_responses module`
   - 例: `χ-7 docs: update conf-reference §4 for openai_max_output_tokens`
5. **Breaking change** は `CHANGELOG.md` の `### Breaking changes (<phase>)` に必ず明記
6. **Phase 仕様書** (`docs/roadmap/phase-<name>-*.md`) は Phase 開始前に書いて PR に含める（UNVET の `docs/plan.md` スタイル）
7. **Phase 進行中**は [`roadmap.md`](roadmap.md) のチェックボックスを各サブフェーズ完了時に tick

---

## 実行入口（計画・工程順）

現状のエントリは `src/main.rs` → `lib::run()` の **1 プロセス構成**。将来的に **CLI 版**と **desktop 版**の 2 実行ファイルを用意し、いずれも **同一コア（ライブラリ）**から起動する薄い runner とする（通常は静的リンク。詳細・名称の正本は [`roadmap/v2-vmc-and-restructure.md`](roadmap/v2-vmc-and-restructure.md) §1.1 / §1.2。再構造化の **Step 4〜9 チェックリスト**は [`roadmap.md`](roadmap.md)「v2 crate / runner / GUI 同梱」。

**推奨する実装順**

1. **crate 再構造化**（`vac-core` 等、`v2` 計画書 §3）— 境界が固まってから runner を増やす。
2. **CLI runner と desktop runner** の詳細設計・実装 — どちらも単体起動可能（CLI はコンソール付き玄人向け、desktop はコンソール非表示・一般ユーザー向け入口）。
3. **Tauri（Phase ε-2）** を **desktop 版に組み込む** — ネイティブウィンドウ／トレイ統合はコンソールを出さない側に寄せる。

**GUI 静的ファイル**: Svelte の **ビルド済み** `gui/dist` を専用 crate またはモジュールに同梱し、CLI／desktop の両方から **内蔵配信**できるようにする（詳細は v2 計画書 §1.2）。

HTTP/WS をそのまま使う Tauri shell 方針は [`roadmap/phase-epsilon-shutdown-and-tauri.md`](roadmap/phase-epsilon-shutdown-and-tauri.md) §3 を参照。

---

## Related Documents

- [`roadmap.md`](roadmap.md) — 全フェーズのチェックボックスとサブフェーズ一覧
- [`roadmap/phase-delta-spec.md`](roadmap/phase-delta-spec.md) — Flowgraph 基幹仕様
- [`manual/index.md`](manual/index.md) — ユーザー向けマニュアル
- [`../CHANGELOG.md`](../CHANGELOG.md) — Phase 単位の変更履歴
