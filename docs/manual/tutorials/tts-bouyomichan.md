# Tutorial: tts-bouyomichan

棒読みちゃん（Bouyomichan）で読み上げる。

> ソース: [`flowgraph.example/tts-bouyomichan/main.flowgraph.toml`](../../../flowgraph.example/tts-bouyomichan/main.flowgraph.toml) + [`conf.example-tts.toml`](../../../conf.example-tts.toml)

## ねらい

- `tts.speak` の `engine = "bouyomichan"` で TCP 接続の棒読みちゃんに読ませる
- `voice` が声番号（数字文字列）の仕様を理解する

## グラフ構造

```
ingress.web_input ─→ tts.speak (engine="bouyomichan",
                                  voice="0"   (0: 女性1, 1: 女性2, 2: 男性1, ...),
                                  speed=100 (50-200), pitch=100, volume=100,
                                  endpoint="127.0.0.1:50001")
                                  ├→ log(audio_path)   ← Bouyomichan は音声を返さないため常に空
                                  └→ log(error)
```

## 前提

- Bouyomichan が起動、TCP ソケット 50001 が待ち受け
- VAC からポート 50001 に疎通可能

## 実行手順

1. `flowgraph.example/tts-bouyomichan/` を `flowgraph/` にコピー
2. `voice` / `endpoint` を必要に応じて書き換え（標準構成ならデフォルトで動く）
3. VAC 再起動 or Reload
4. <http://127.0.0.1:57000/input> に文字列 → 棒読みちゃんが読み上げ

## 注意

- 棒読みちゃんは「VAC 側で wav を掴む」仕組みではなく**棒読みちゃん本体が直接再生**するため、`audio_path` は常に空文字列になる
- `save_path` を使っても棒読みちゃん側には保存機能がないため無視される

## 関連ノード

[`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak)
