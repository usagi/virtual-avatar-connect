# Quickstart — v2 を 5 分で立ち上げる

このドキュメントは **v2 配布物を新規に動かす**手順です。v1 からの移行は [README](../../README.md) と `migrate` CLI（δ-8d 提供予定）を参照してください。

---

## 0. 前提

- Windows 10 / 11（Linux / macOS も基本動作するが、Screenshot / OCR は Windows 専用）
- [Rust stable](https://www.rust-lang.org/tools/install)（1.75+）
- [Node.js LTS](https://nodejs.org/ja)（GUI ビルド用）
- （任意）VOICEVOX / AivisSpeech / CoeiroInk / 棒読みちゃん / OBS Studio 等

---

## 1. ビルド

```powershell
git clone https://github.com/usagi/virtual-avatar-connect.git
cd virtual-avatar-connect

# Rust 本体（Vosk 込みの既定ビルド）
cargo build --release

# GUI (Svelte)
cd gui
npm install
npm run build
cd ..
```

※ Whisper も使いたい場合は `cargo build --release --features voice-whisper`（要 CMake / LLVM / C++）。

---

## 2. 初回起動

```powershell
.\target\release\virtual-avatar-connect.exe
```

既定で以下が有効になります。

| 項目 | 値 |
|---|---|
| Web UI | <http://127.0.0.1:57000/gui/> |
| Control API | <http://127.0.0.1:57000/api/v1/control/> |
| WebSocket | `ws://127.0.0.1:57000/ws/control` |
| Input 投稿 UI | <http://127.0.0.1:57000/input> |
| OBS 字幕 | <http://127.0.0.1:57000/output/subtitles-1> 等 |

起動時に自動生成される Bearer token は `<runtime_dir>/control-token.txt` に書き出されます。GUI はこのファイルを自動で読むので通常は意識不要です。

---

## 3. 最小の Flowgraph を動かす

`flowgraph.example/main.flowgraph.toml`（配布済み）は web_input を受けて log するだけの最小サンプルです。

```powershell
copy flowgraph.example flowgraph -Recurse
```

もう一度 VAC を起動し直すと、`flowgraph/` 配下のすべての `*.flowgraph.toml` が自動ロードされます（`flowgraph_dir` で場所変更可）。

### GUI で触る

1. <http://127.0.0.1:57000/gui/> を開く
2. 上部タブから **Flowgraph** を選択
3. 左ペインのツリーで `main.flowgraph.toml` をクリック → 中央にグラフが表示
4. <http://127.0.0.1:57000/input> に適当な文字列を投げると、Flowgraph の `in → log` が発火し、ターミナルに `[util.log] ...` と出る

### 編集してみる

- 中央のグラフ上の `log` ノードをドラッグ移動 → 左ペインに差分が出る
- 右ペインのパレットから `flowgraph.tts.speak` を配置
- `log` の前/後段に繋ぎ直し → 右ペイン下の「Save」で保存
- 保存で即 reload され WebSocket で全クライアントへ伝播

---

## 4. よく使うシナリオへ進む

まずは目的に近いチュートリアルから:

| やりたいこと | 参照 |
|---|---|
| Twitch チャットを TTS で読み上げ | [twitch-echo](./tutorials/twitch-echo.md) → [tts-voicevox](./tutorials/tts-voicevox.md) |
| マイクに話しかけて AI が返す | [voice-input](./tutorials/voice-input.md) → [openai-persona](./tutorials/openai-persona.md) |
| /コマンドで OBS シーンを切り替え | [chat-filters](./tutorials/chat-filters.md) |
| 翻訳字幕を 2 言語で出す | [translate-multilang](./tutorials/translate-multilang.md) |
| ゲーム画面の OCR を AI に読ませる | [ocr-screencap](./tutorials/ocr-screencap.md) |

各 conf 設定の詳細は [conf-reference.md](./conf-reference.md)、全ノードの spec と effect / capability / state metadata の逆引きは [node-catalog.md](./node-catalog.md) を参照。

---

## 5. トラブルシューティング

| 症状 | 対処 |
|---|---|
| GUI を開くと「ビルド手順を案内する HTML」が出る | `gui/dist` がない。`cd gui && npm install && npm run build` |
| `/api/v1/control/*` が 401 | `control-token.txt` を読んで `Authorization: Bearer <token>` を付ける（LAN 越しの場合） |
| Flowgraph の diagnostics に「型不一致」 | edge の source / target の型が違う。SocketType の表記は [node-catalog.md](./node-catalog.md) を参照 |
| Twitch の OAuth が始まらない | GUI の Twitch タブから Device Code Flow を開始、`conf.example-twitch.toml` の手順に従う |
| Vosk DLL が見つからない | `VOSK_WIN64_VERSION` / `VOSK_LIB_PATH` 環境変数。詳細は `conf.example-voice.toml` |

---

次: [conf-reference.md](./conf-reference.md) / [node-catalog.md](./node-catalog.md) / [tutorials/](./tutorials/)
