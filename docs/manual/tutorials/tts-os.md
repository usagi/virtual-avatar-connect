# Tutorial: tts-os

OS 標準の TTS（Windows SAPI / macOS say / Linux の tts crate バックエンド）で読み上げる最小サンプル。

> ソース: [`flowgraph.example/tts-os/main.flowgraph.toml`](../../../flowgraph.example/tts-os/main.flowgraph.toml) + [`conf.example-tts.toml`](../../../conf.example-tts.toml)

## ねらい

- `flowgraph.tts.speak` の `engine = "os"` の最小配線
- 特別な外部エンジン無しで音声出力を確認する

## グラフ構造

```
ingress.web_input ─→ tts.speak (engine="os", voice="")
                                 ├→ log(audio_path)
                                 └→ log(error)
```

## 前提

- Windows / macOS なら特に何も要らない
- Linux はディストリビューションの TTS の有無に依存

## 実行手順

1. `flowgraph.example/tts-os/` を `flowgraph/` にコピー
2. VAC 再起動 or Reload
3. <http://127.0.0.1:57000/input> に文字列 → スピーカーから読み上げ

## 声を変える

- Windows: `--test-os-tts` で使えるボイス一覧を出し、`voice` literal を書き換え
- macOS: `say -v ?` で一覧、同様に `voice` literal を書き換え

## 応用

- 速度 / ピッチ / 音量を `literal.float` で明示配線
- `save_path` を `literal.string` で渡すと wav ファイル出力もできる

## 関連ノード

[`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak) / [`flowgraph.ingress.web_input`](../node-catalog.md#flowgraph-ingress-web-input)
