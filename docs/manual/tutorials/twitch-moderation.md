# Tutorial: twitch-moderation

Web 経由の ad hoc トリガーから特定視聴者を 10 分 timeout する管理者向けサンプル。

> ソース: [`flowgraph.example/twitch-moderation/main.flowgraph.toml`](../../../flowgraph.example/twitch-moderation/main.flowgraph.toml) + [`conf.example-twitch.toml`](../../../conf.example-twitch.toml)

## ねらい

- `twitch.user_id_by_login`（login → user_id 解決）と `twitch.timeout` の連携
- モデレーター専用スコープ（`moderator:manage:banned_users`）のトークン利用を確認

## グラフ構造

```
ingress.web_input ─→ twitch.user_id_by_login (login=web_input の content)
                       ├─ on_success ─→ twitch.timeout (duration=600s, reason="...")
                       └─ on_error  ─→ util.log
```

## 前提

- `[twitch.moderator]` で Device Code Flow が完了（モデレーター scope 付き）
- 自分が broadcaster の channel に対してモデレーション権限を持っている

## ⚠ セキュリティ上の暫定仕様

[twitch-chat-send](./twitch-chat-send.md) と同様、トークン類を literal に直書きしている。共有時は必ず該当ノードを削除してから export すること（δ-9 の `flowgraph.token_source.twitch` で根本解決予定）。

## 実行手順

1. `flowgraph.example/twitch-moderation/` を `flowgraph/` にコピー
2. `literal.string` 群（access_token / client_id / broadcaster_id / moderator_id）をモデレータートークンで書き換え
3. VAC 再起動 or Reload
4. <http://127.0.0.1:57000/input> にタイムアウトしたい視聴者の login 名（小文字）を投稿
5. 当該ユーザーが 10 分 timeout される

## よくある拡張

- `twitch.timeout` の代わりに `twitch.ban`（永久 BAN）に差し替え
- duration / reason を上流で動的に組み立て
- トリガー元を `ingress.twitch`（特定コマンド `/timeout <login>`）に変える

## 関連ノード

[`flowgraph.twitch.user_id_by_login`](../node-catalog.md#flowgraph-twitch-user-id-by-login) / [`flowgraph.twitch.timeout`](../node-catalog.md#flowgraph-twitch-timeout) / [`flowgraph.twitch.ban`](../node-catalog.md#flowgraph-twitch-ban)
