# Virtual Avatar Connect — Manual (v2)

VAC v2 のユーザー向けマニュアル。

v2 では「チャンネル名で繋ぐ v1 プロセッサー列」から **Flowgraph DAG**（型付きソケットと exec 線で配線するノードグラフ）へ移行しました。本マニュアルは v2 の使い方と利用可能なノード一覧の**正本**です。

> v1 から乗り換える場合は後述 [CLI ツール](#cli-ツール) の `migrate` サブコマンドで best-effort 変換を試せます（δ-8d で提供予定）。

---

## 目次

### はじめに
- [Quickstart](./quickstart.md) — v2 の初回起動から「発話 → 字幕 → TTS」まで 5 分で動かす
- [conf.toml リファレンス](./conf-reference.md) — v2 配布 `conf.toml` のキー一覧
- [Node Catalog](./node-catalog.md) — 組み込みノードの全ポート・全プロパティ（自動生成）
- [Dimensional Quantity System](./dimensional-quantity-system.md) — SI 準拠の単位次元システム（Phase ξ）。数値に単位を貼り付け、次元不一致をエンジンで検知する
- [DateTime System](./datetime-system.md) — 絶対時刻の `datetime` 型と `flowgraph.datetime.*` ノード（Phase π）。Duration は `quantity`（時間次元）に統一
- [Flowgraph Enum & Library](./flowgraph-enum-and-library.md) — 閉集合 `string`、`[[enums]]`、ライブラリ境界ノード、`library_uses`（Phase λ）

### Tutorials（各 `flowgraph.example/` の解説）

- [chat-echo](./tutorials/chat-echo.md)：最小の echo（ingress.web_input → util.log）
- [chat-filters](./tutorials/chat-filters.md)：command.match / dictionary.replace / tts.speak
- [ocr-screencap](./tutorials/ocr-screencap.md)：screenshot.capture → ocr.recognize
- [openai-persona](./tutorials/openai-persona.md)：AI Persona × Flowgraph の接続
- [translate-multilang](./tutorials/translate-multilang.md)：GAS と Libre の並列翻訳
- [tts-os](./tutorials/tts-os.md)：OS TTS
- [tts-voicevox](./tutorials/tts-voicevox.md)：VOICEVOX / AivisSpeech
- [tts-coeiroink](./tutorials/tts-coeiroink.md)：CoeiroInk
- [tts-bouyomichan](./tutorials/tts-bouyomichan.md)：棒読みちゃん
- [twitch-echo](./tutorials/twitch-echo.md)：Twitch チャット取り込み
- [twitch-chat-send](./tutorials/twitch-chat-send.md)：rate_limit + chat_send
- [twitch-moderation](./tutorials/twitch-moderation.md)：user_id_by_login + timeout
- [voice-input](./tutorials/voice-input.md)：Vosk / Whisper 認識 → TTS echo
- [dictionary-editor-and-quick-add](./tutorials/dictionary-editor-and-quick-add.md)：GUI から TSV 用語集を直接編集する / Live Quick-Add で `glossary.learn` を即発火する（Phase φ / GRN）
- [gui-e2e](./tutorials/gui-e2e.md)：Playwright で GUI の回帰テストを実行する / fixture 構成と失敗時の切り分け（Phase ν）
- [flowgraph-fixtures](./tutorials/flowgraph-fixtures.md)：CLI から Flowgraph fixture を実行し、mock IO と recorded effects で言語コアを検証する

---

## v2 アーキテクチャの前提

### 3 つの実行層

```
┌───────────────────────────────────────────────────────────┐
│ 1. Service layer（conf.toml が設定する常駐サービス）      │
│    - AI Persona（OpenAI Chat）                             │
│    - Twitch（OAuth / EventSub / Chat Send）                │
│    - Voice（Vosk / Whisper）※ δ-9 で [voice] サービス化   │
│    - LibreTranslate / Screenshot / OCR の補助設定         │
│    - run_with（外部プロセス起動）                          │
└───────────────────────────────────────────────────────────┘
                              ↓ TriggerSource
┌───────────────────────────────────────────────────────────┐
│ 2. Flowgraph layer（flowgraph.example/*.flowgraph.toml）  │
│    - ingress.{web_input,voice,twitch} で外部入力を受ける  │
│    - Pure / Stateful / Effectful ノードで処理             │
│    - util.log / tts.speak / twitch.chat_send で出力       │
└───────────────────────────────────────────────────────────┘
                              ↓
┌───────────────────────────────────────────────────────────┐
│ 3. UI layer（GUI / OBS Browser Source）                    │
│    - Svelte GUI（/index.html）: Flowgraph 編集 + 制御      │
│    - /output/subtitles-*, /output/effects, /output/bgm    │
└───────────────────────────────────────────────────────────┘
```

### Flowgraph 用語ミニ辞書

| 用語 | 意味 |
|---|---|
| **Feature** | ノードの種類を示す文字列 ID。例: `flowgraph.tts.speak` |
| **Socket** | 入出力端子。型は `bool` / `int` / `float` / `string` / `json` / `list<T>` / `map<T>` / `table` / `quantity` / `datetime` / `exec`。`quantity` は [Dimensional Quantity System](./dimensional-quantity-system.md)、`datetime` は [DateTime System](./datetime-system.md) |
| **Port** | ノードに所属する個別のソケット（name + type + exec/data）|
| **Edge** | `from = "node_id:port"` → `to = "node_id:port"` の TOML 記述 |
| **Exec 線** | 処理の発火タイミングを伝える制御フロー（`exec_in` / `exec_out`）|
| **Property** | ノード外部では見えないが、ノード自身が使う静的設定（literal ノードの `value` など）|
| **Pure / Stateful / Effectful** | 副作用クラス。PureNode は I/O 不可、StatefulNode は状態あり I/O なし、EffectfulNode のみ I/O 可 |

### 3 つの設定ファイル

1. **`conf.toml`** — サービス設定のみ。`[[processors]]` は v2 では不可（voice のみ暫定例外、δ-9 で移行）。
2. **`flowgraph.example/*.flowgraph.toml`** — DAG 本体。`[meta]` / `[[nodes]]` / `[[edges]]` の 3 種類のテーブル。
3. **`dictionary.*.txt` / `regex.*.txt`** — 辞書ファイル（v2 では flowgraph ノードの入力として渡すが、永続化用 fs ノードは δ-9）。

---

## CLI ツール

（δ-8d で提供予定）

- `virtual-avatar-connect.exe` — 通常起動（`conf.toml` を読み込んで Service + Flowgraph を起動）
- `--coeiroink-speakers`, `--voicevox-speakers`, `--aivisspeech-speakers` — 音声エンジンの voice 一覧ダンプ
- `--test-os-tts` — OS TTS のエンジン一覧ダンプ
- `--openai-chat-fine-tuning` — AI Persona のファインチューニングジョブ実行
- `--openai-api-clear-files` — OpenAI API に残留するアップロードファイルを一括削除
- `migrate`（δ-8d 予定）— 旧 v1 `conf.toml` を v2 形式へ best-effort 変換

---

## 関連リンク

- [roadmap/phase-delta-spec.md](../roadmap/phase-delta-spec.md) — v2 移行ロードマップ（設計・進捗の一次情報）
- [README.md](../../README.md) — プロジェクト概要
- GitHub: <https://github.com/usagi/virtual-avatar-connect>
