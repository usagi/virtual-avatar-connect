# Tutorial: twitch-chat-send

Twitch のコメントを受けて、`[echo] <内容>` というプレフィックスを付けて返信するエコーボット。

> ソース: [`flowgraph.example/twitch-chat-send/main.flowgraph.toml`](../../../flowgraph.example/twitch-chat-send/main.flowgraph.toml) + [`conf.example-twitch.toml`](../../../conf.example-twitch.toml)

## ねらい

- `string.concat` / `util.rate_limit` / `twitch.chat_send` の配線を学ぶ
- Twitch API トークン類を **literal として一旦直書き**する（暫定）の注意点を理解する

## グラフ構造

```
ingress.twitch ─→ string.concat ("[echo] ", content) ─→ util.rate_limit (count=1/10s)
                                                               ↓
                                                         twitch.chat_send (access_token, client_id,
                                                                             broadcaster_id, sender_user_id)
                                                               ├→ log(on_success)
                                                               └→ log(on_error)
```

## 前提

- Twitch OAuth（moderator / broadcaster）完了
- chat scope (`user:write:chat` または `chat:edit`) のトークン

## ⚠ セキュリティ上の暫定仕様

`twitch.chat_send` の `access_token` / `client_id` / `broadcaster_id` / `sender_user_id` を **`flowgraph.literal.string` に直書き**している。この flowgraph ファイルをそのまま共有すると **トークンが漏れる**ので注意。

正式には δ-9 で `flowgraph.token_source.twitch`（Control API 経由でトークンを参照、TOML には ID のみ書く）のような専用ノードを入れる予定。

## 実行手順

1. `flowgraph.example/twitch-chat-send/` を `flowgraph/` にコピー
2. `literal.string` 4 本（access_token / client_id / broadcaster_id / sender_user_id）をトークン値に書き換え
3. VAC 再起動 or Reload
4. Twitch チャットに何か書く → ボットが `[echo] <内容>` を返す

## よくある拡張

- `util.rate_limit` の `count` / `window_seconds` で頻度を調整
- `string.concat` の代わりに `regex.replace` で複雑整形
- プレフィックスをコマンドで変更可能にする → [chat-filters](./chat-filters.md)

## 関連ノード

[`flowgraph.ingress.twitch`](../node-catalog.md#flowgraph-ingress-twitch) / [`flowgraph.string.concat`](../node-catalog.md#flowgraph-string-concat) / [`flowgraph.util.rate_limit`](../node-catalog.md#flowgraph-util-rate-limit) / [`flowgraph.twitch.chat_send`](../node-catalog.md#flowgraph-twitch-chat-send)
