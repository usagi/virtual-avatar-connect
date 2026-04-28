# Tutorial: chat-filters

v1 の Command / Modify / DictionaryCommand を v2 Flowgraph で置き換える最小サンプル。

> ソース: [`flowgraph.example/chat-filters/main.flowgraph.toml`](../../../flowgraph.example/chat-filters/main.flowgraph.toml)

## ねらい

- `command.match` で `/prefix` のスラッシュコマンドとそれ以外を分岐
- `dictionary.replace` で辞書置換
- `tts.speak`（OS）に流す経路と、コマンドをログに落とす経路を分離

## グラフ構造

```
ingress.web_input ─→ command.match (prefix="/")
                      ├─ on_command ─→ util.log (command)
                      └─ on_other  ─→ dictionary.replace ─→ tts.speak(os)
                                                               ├→ log(audio_path)
                                                               └→ log(error)
```

## 前提

- VAC 起動中
- OS TTS が動く環境（Windows/Mac）

## 実行手順

1. `flowgraph.example/chat-filters/` を `flowgraph/` にコピー
2. VAC 再起動 or Reload
3. `http://127.0.0.1:57000/input` に普通の文字列 → OS TTS が読み上げ
4. `/ping hello world` のように `/` で始めた入力 → 「command=ping」がログに出るだけ（読み上げなし）

## 現時点の限界

- `glossary.replace` の `dictionary` 入力は 11 カラム Table を期待する。本例では既定の空用語集のまま。
- `cmd:args` は `list<string>`、`util.log:value` は `string` のため直接接続不可

## 応用

- `on_command` から `util.log` の代わりに `tts.speak` や `twitch.chat_send` を繋ぐ → コマンド応答
- 後段に `regex.replace` を差し込んで禁止語伏せ字化
- 別系統に `glossary.match` / `glossary.learn` / `glossary.forget` を噛ませて「学習(X:=Y)」「忘却(X)」をチャットから受け付ける

## 関連ノード

[`flowgraph.command.match`](../node-catalog.md#flowgraph-command-match) / [`flowgraph.glossary.replace`](../node-catalog.md#flowgraph-glossary-replace) / [`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak)
