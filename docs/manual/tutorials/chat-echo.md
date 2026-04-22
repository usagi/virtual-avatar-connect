# Tutorial: chat-echo

Twitch コメントを sibling の TTS flowgraph に橋渡しする **multi-file サンプル**。

> ソース: [`flowgraph.example/chat-echo/main.flowgraph.toml`](../../../flowgraph.example/chat-echo/main.flowgraph.toml) + [`tts.flowgraph.toml`](../../../flowgraph.example/chat-echo/tts.flowgraph.toml)

## ねらい

- `flowgraph_dir` 配下の **複数ファイル間で edge を張る**方法（cross-file edge）を学ぶ
- `ingress.twitch` を使う最小サンプル
- fq name（`./tts::speaker` のような sibling 参照）の書き方を押さえる

## グラフ構造

```
[chat-echo/main.flowgraph.toml]
  ingress.twitch:exec_out  ─┐
  ingress.twitch:content    │
                            ├→  [chat-echo/tts.flowgraph.toml]
                            │     speaker:exec_in
                            └→    speaker:text
```

`from` / `to` の書式:

```toml
[[edges]]
from = "in:exec_out"
to = "./tts::speaker:exec_in"
```

- `./tts` は同ディレクトリの `tts.flowgraph.toml` を指す
- `::speaker` はそのファイル内の node id
- `:exec_in` は port 名

## 前提

- `conf.example-twitch.toml` を見て Twitch OAuth が完了している
- ベースの `main::speaker` は `tts.speak`（engine=`os`）に向けて既に configure 済み

## 実行手順

1. `flowgraph/chat-echo/` をディレクトリごと配置
2. VAC 再起動（or GUI の「Reload」）
3. 配信チャットに発言すると、OS TTS が読み上げる

## よくある拡張

- `tts.flowgraph.toml` の中身を voicevox や coeiroink に差し替え（[tts-voicevox](./tts-voicevox.md) 参照）
- `in:sender_actor` / `source_kind` を使って「モデレーターだけ読む」などの分岐

## 関連ノード

[`flowgraph.ingress.twitch`](../node-catalog.md#flowgraph-ingress-twitch) / [`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak)
