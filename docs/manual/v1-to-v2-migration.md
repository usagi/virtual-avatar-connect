# v1 → v2 移行ガイド

v0.9.x 系（V1 processor + Flowgraph 並走）から v0.10.0（Flowgraph-only、v2）への移行手順を
まとめる。ここでの「V1」は `[[processors]]` と V1 Control API、「V2」は `flowgraph_dir`
配下の Flowgraph Runtime を指す。

## 概要

v0.10.0 では **V1 processor 層が完全撤去**された。`[[processors]]` は TOML には残っても
無視され、起動時に deprecation 警告が出る。すべての既存機能は Flowgraph のノードで置き換え
られている。

| v1 の feature                  | v2 での代替                              | 備考                                                      |
| ------------------------------ | ---------------------------------------- | --------------------------------------------------------- |
| `feature = "twitch"`           | `flowgraph.ingress.twitch` / `twitch_eventsub` | `token_key` 指定で DCF 永続トークンを使う。             |
| `feature = "twitch-out"`       | `flowgraph.twitch.chat_send`             | モデレーターボット発話も兼ねる。                          |
| `feature = "command"`          | `flowgraph.command.match` + `flowgraph.command.set` | scene switcher は `command.set` の `sets` prop で表現。 |
| `feature = "dictionary"` 系    | `flowgraph.glossary.replace` / 旧 `dictionary.command` 相当 | 既存 `dictionary.*.txt` はそのまま読み込まれる。     |
| `feature = "coeiroink"`/`voicevox`/`aivis_speech`/`bouyomichan`/`os_tts` | `flowgraph.tts.speak` | `engine = "coeiroink"` 等で切り替え。 |
| `feature = "gas-translation"`  | `flowgraph.translate.gas`                |                                                           |
| `feature = "libre-translation"`| `flowgraph.translate.libre`              |                                                           |
| `feature = "screenshot"`/`ocr` | `flowgraph.screenshot.capture` + `flowgraph.ocr.recognize` |                                               |
| `feature = "modify"`           | `flowgraph.glossary.replace` / `flowgraph.regex.*` | 直接チェインしてパイプ化する。                        |

## 手順

### 1. バックアップ

まず `conf.toml` / `conf.local.*.toml` / `dictionary.*.txt` / `flowgraph.example/` を
リポジトリ/任意の場所にコピーしておく。

### 2. `flowgraph_dir` を設定

```toml
# conf.toml
flowgraph_dir = "flowgraph"   # 任意のディレクトリ
```

ここに `.flowgraph.toml` ファイルを配置する。初心者は `flowgraph.example/` 配下の
サンプル（`chat-echo`, `tts-coeiroink`, `twitch-events`, `command-sets` 等）を
コピーして使うのが近道。

### 3. `[[processors]]` を Flowgraph に置換

各 processor エントリに対応する Flowgraph ノードを `flowgraph_dir` に追加する。

- ingress: `flowgraph.ingress.web_input` / `voice` / `twitch` / `twitch_eventsub` /
  `channel_subscribe` のいずれかでチャンネルを引き込む。
- 変換層: `flowgraph.glossary.replace` / `flowgraph.regex.replace` /
  `flowgraph.translate.*` 等でテキスト加工。
- 出口: `flowgraph.tts.speak` / `flowgraph.twitch.chat_send` /
  `flowgraph.channel.emit`（browser-output / WS 配信）など。

### 4. Twitch 設定の統合

- v2 では `[twitch]` + `[twitch.eventsub]` + `[twitch.moderator]` の 3 セクション編成に
  なっている。`token_key` で「どの DCF セッションのトークンを使うか」をノードごとに選べる。
- `broadcaster_login` を `[twitch.eventsub]` と `[twitch]` で揃えると、
  Flowgraph に `flowgraph.ingress.twitch_eventsub` がある場合は V1 EventSub ループが
  自動スキップされる（ζ-2c）。v1 と同じ挙動に固定したい場合は
  `[twitch.eventsub].force_v1_loop = true`。

### 5. AI ペルソナ（`[[ai.personas]]`）

- `[[ai.personas]]` は v2 でも常駐タスクとして残る。Flowgraph とは別ラインで動く。
- `vac_twitch_chat_say` ツールを使う場合は `custom_instructions` に行動規範を書いておくこと
  （`conf.example-openai-chat.toml` 参照）。AI が通常発話の代わりに Twitch へ直接
  打ち込んでしまう事故を防ぐ。

### 6. `channel_to` の扱い

V1 の `channel_to` は Flowgraph では「どのチャンネルへ `channel.emit` するか」で
表現される。将来の版で TOML 層からも削除予定。暫定的に `[[processors]]` が残っていても
ランタイム上は参照されない。

### 7. GUI での編集

v2 では GUI の `Flowgraph` タブで:

- ファイルツリーからノードセットを選んで編集
- ノードパレット（右上）からドラッグ追加
- プロパティエディタ（右下）で string/int/float/bool/json/list/map を編集
- Ctrl+S で保存、未保存時は `Save *`（warning 色）
- Delete / Backspace で選択削除（undo トースト付き）

## デバッグ

- `GET /api/v1/control/flowgraph/diagnostics`：現在の graph 診断。
- GUI の Diagnostics パネルで error / warning / info 件数とメッセージを確認。
- `reload` API（`POST /api/v1/control/flowgraph/reload`）でディスクから再読込。
  bridges も自動で respawn される。web_input endpoint の追加／削除時だけは
  プロセス再起動を促す toast が出る。

## 廃止された CLI フラグ

- `--coeiroink-speakers` / `--aivisspeech-speakers` / `--voicevox-speakers` / `--test-os-tts`
  は v0.9.x で一時停止、v2 では未再実装。TTS ドライバ経由で δ-9.3 にて再導入予定。

## 関連ドキュメント

- [`docs/manual/node-catalog.md`](node-catalog.md): 全ノードの入出力/プロパティ一覧
- [`docs/manual/quickstart.md`](quickstart.md): 起動からの導線
- [`CHANGELOG.md`](../../CHANGELOG.md): 0.9.x → 0.10.0 の差分
- `flowgraph.example/`: 実働サンプル
