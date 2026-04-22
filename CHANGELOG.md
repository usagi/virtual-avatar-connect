# Changelog

本ファイルは VAC (Virtual Avatar Connect) の主要な変更履歴を Phase 単位で記録する。
個別の詳細設計は `docs/roadmap/phase-delta-spec.md` を参照。

## [Unreleased]

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
