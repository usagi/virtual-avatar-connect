# Tutorial: twitch-echo

Twitch チャットを VAC に取り込んで util.log で確認する最小サンプル。

> ソース: [`flowgraph.example/twitch-echo/main.flowgraph.toml`](../../../flowgraph.example/twitch-echo/main.flowgraph.toml) + [`conf.example-twitch.toml`](../../../conf.example-twitch.toml)

## ねらい

- `ingress.twitch` の trigger source としての動作を確認
- Twitch OAuth まわりの設定が一通り揃っているか検証する踏み台として使う

## グラフ構造

```
ingress.twitch ─→ util.log
```

## 前提

- `conf.example-twitch.toml` を参考に `[twitch]` / `[twitch.eventsub]` を conf.toml に用意
- GUI の Twitch タブ（あるいは `POST /api/v1/control/oauth/twitch/broadcaster/start`）で Device Code Flow を完了

## 実行手順

1. `flowgraph.example/twitch-echo/` を `flowgraph/` にコピー
2. Twitch OAuth 済みの状態で VAC 起動
3. 配信チャンネルにコメントを投稿
4. VAC ログに `[util.log] <コメント>` が出る

## よくある拡張

- `util.log` を `tts.speak` に差し替え → コメント読み上げボット（[tts-voicevox](./tts-voicevox.md) 参照）
- `ingress.twitch:source_kind` で `chat` / `follow` / `subscribe` 等を分岐

## 関連ノード

[`flowgraph.ingress.twitch`](../node-catalog.md#flowgraph-ingress-twitch) / [`flowgraph.util.log`](../node-catalog.md#flowgraph-util-log)
