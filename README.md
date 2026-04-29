# Virtual Avatar Connect

配信支援アプリ Virtual Avatar Connect; VAC

AI疑似人格共演者、VRM制御、音声認識、画像認識、字幕、翻訳、読み上げなどなど

- ここは開発プロジェクトとしての公式ウェブサイト <https://github.com/usagi/virtual-avatar-connect> です。
  - ご要望や不具合の報告、OSSとしてのプロジェクトへのご参加はこちらからどうぞ。🙇🏼‍♀️
- 通常の閲覧用の公式ウェブサイトは <https://usagi.github.io/virtual-avatar-connect/> です。
  - 通常はこちらへアクセスして下さい。🙏

## ご注意: VAC は現在開発途中のα版となっております🙏

- 基本機能は動作しますが、まだたくさんの不具合があるかもしれません。🙇🏼‍♀️
  - 不具合を見つけたら [Issues] へご報告ください。
    - 「1. 動作環境」、「2. 再現手順」、「3. 期待した動作」の3点をご報告下さい。
    - 設定やスクショを貼る場合は API-KEY などの漏らしたくない情報が含まれないようご注意下さい。
  - ご要望も [Issues] へどうぞ！

[Issues]:https://github.com/usagi/virtual-avatar-connect/issues

## v1 → v2 移行

v0.10.0 で **V1 processor 層は撤去**されました。`[[processors]]` は TOML に残っても無視
されます。Flowgraph への移行は [`docs/manual/v1-to-v2-migration.md`](docs/manual/v1-to-v2-migration.md)
を参照してください。主要な変更点の一覧は [`CHANGELOG.md`](CHANGELOG.md) にあります。

## Roadmap (抜粋)

現在は Phase VI: **GUI 化** を進めています。GUI 本体に先立ち、まず制御面を REST/WS として切り出します。

- **Phase VI-α: Control API 層**（いま進行中）
  - `/api/v1/control/*` に REST / WebSocket を集約する認証付きコントロールプレーン。
    - `GET /ping` / `GET /whoami` — 疎通・認証ポリシー診断
    - `GET /snapshot` — processors / ai_personas / twitch の現状 JSON
    - `POST /pause` / `POST /resume` — processor / AI persona の soft pause（全体・id/index 指定）
    - `POST /reload` — AI persona の `custom_instructions` / `system_instructions_extra` / `heartbeat.enabled` / `decision.threshold` と、`modify` プロセッサの辞書ファイルを無停止で差し替え
    - `POST /oauth/twitch/{broadcaster|moderator}/start|cancel` / `GET /status` — Twitch Device Code Flow を GUI ボタンからオンデマンドで起動・追跡・取消（起動時ブロックを避けるための仕組み）
    - `POST /ingress` — 任意の `ChannelDatum` をパイプラインに投入（GUI からの発話テスト等、`[[processors]] feature="webinput"` 非依存）
    - `GET /profiles` / `POST /restart` — 同ディレクトリの `conf.*.toml` を列挙し、GUI からワンクリックでプロファイル切替再起動（Phase VI-γ-1）
    - `GET /events` — WebSocket で `ChannelDatum` / `pause_state` / `reloaded` / `oauth_status` / `processor_invoked` / `restarting` / `heartbeat` / `lagged` を一本ストリームで購読
  - 既存の `/resources/…`・`/output/…`（OBS ブラウザソース）や現行 UI の `/websocket` は引き続き無保護で維持（OBS に Bearer を埋めるのは現実的でないため）。
  - 設定: [`conf.example-control-api.toml`](conf.example-control-api.toml)
  - 認証モデル: 「bind 先は `web_ui_address`」「その接続に Bearer が要るかは `[control_api]` のポリシー」の二軸分離。
    - 同 PC (loopback) は既定無認証で、LAN からは Bearer 必須。フルトラスト LAN は設定で緩和可。
    - WebSocket は `Authorization: Bearer …` ヘッダに加え `?token=…` クエリでも認証可能（ブラウザの `new WebSocket()` 向け）。
  - 動作確認用 [Bruno](https://www.usebruno.com/) collection: [`dev/bruno/vac-control-api/`](dev/bruno/vac-control-api/README.md)
- **Phase VI-β: Web GUI 本体**（着手中）
  - スタック: Svelte 5 (runes) + Vite + TypeScript + [Svelte Flow](https://svelteflow.dev/) + [Skeleton v4](https://skeleton.dev/) + Tailwind v4 + melt-ui。
  - 詳細と開発手順: [`gui/README.md`](gui/README.md)
  - ダッシュボード、ログ/イベントビューア、設定 UI、node ベースの pipeline エディタ、Kill Switch / Dry-run。
- Phase VI-γ: 可視化・リプレイ（AI Decision スコア内訳、イベントリプレイ、タイムラインバー）。
- Phase VII: モデレーター定型機能（RAID 応答・サブ/cheer 謝意・BAN エスカレーション）。
- Phase VIII (将来): Tauri 化（ブラウザレス・デスクトップ GUI）。

Phase VI 以降は破壊的な設定変更がしばらく続きます。ご注意ください。🙏

## LICENSE

Virtual Avatar Connect は **MIT ライセンス**のオープンソースソフトウェアです。

- [MIT License](LICENSE)

third-party なライブラリ・外部サービスはそれぞれのライセンス/利用規約に従います（主要な依存は `Cargo.toml` 参照）。

## Contributing

バグ報告・機能要望・PR はいずれも [Issues] / Pull Requests からどうぞ。設計方針や大きな破壊的変更の相談は Issue 起票 → 合意 → PR の順が行き違いが少なく助かります。

## Author

- Usagi Ito / USAGI.NETWORK
  - [Twitch](https://www.twitch.tv/usaginetwork)
  - [X(Twitter)](https://twitter.com/usagi_network)
  - [BOOTH](https://usagi-network.booth.pm/)
