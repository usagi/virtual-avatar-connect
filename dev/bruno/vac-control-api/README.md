# VAC Control API — Bruno Collection

[Virtual Avatar Connect](../../..) の Phase VI-α [Control API](../../../src/web_interface/control) を
[Bruno](https://www.usebruno.com/) で叩くための collection です。

## 開き方

1. Bruno をインストール（[Download](https://www.usebruno.com/downloads)）
2. Bruno の「Open Collection」でこのフォルダ（`dev/bruno/vac-control-api`）を選ぶ
3. 右上の環境セレクタで `Local` を選ぶ（LAN 経由で試したい場合は `LAN` を選んで `baseUrl` を編集）
4. VAC を起動してから順番に叩く

## 含まれるリクエスト

- `Health/`
  - **Ping** — `GET /api/v1/control/ping`（疎通確認）
  - **WhoAmI** — `GET /api/v1/control/whoami`（peer_addr / 認証ポリシー診断）
- `State/`
  - **Snapshot** — `GET /api/v1/control/snapshot`（processors / ai_personas / twitch の現状 JSON）
- `Pause/`
  - **Pause - All / Processors / AIs** — 全体・カテゴリ単位の soft pause
  - **Pause - AI by id / Processor by id** — id 指定の soft pause（body を書き換えて使う）
  - **Resume - All / AI by id / Processor by id** — 同じ形で解除
- `Reload/`（Phase VI-α-4）
  - **Reload - AI custom_instructions** — `POST /reload` で `custom_instructions` / `system_instructions_extra` を無停止差し替え
  - **Reload - AI heartbeat** — `heartbeat.enabled` を無停止で ON/OFF
  - **Reload - AI decision threshold** — `decision.threshold` を無停止で調整
  - **Reload - Modify files** — `feature = "modify"` の辞書/正規表現ファイルを再読込（全対象 or `id` 指定）
- `OAuth/`（Phase VI-α-5）
  - **OAuth - Twitch broadcaster start / status / cancel / delete tokens** — broadcaster 用 Twitch Device Code Flow を GUI ボタンから起動／追跡／取消／トークン破棄
  - **OAuth - Twitch moderator start / status / cancel / delete tokens** — bot/moderator アカウント用の同セット。`oauth_browser_command` と組み合わせて別プロファイル起動
- `Ingress/`（Phase VI-β-7）
  - **Ingress - minimal** — `POST /ingress` で最小構成の `ChannelDatum` を投入
  - **Ingress - full payload** — `source` / `meta` / `flags` / `is_final` まで全部埋めた版
- `Restart/`（Phase VI-γ-1）
  - **Profiles** — `GET /profiles`（同ディレクトリの `*.toml` 一覧 + 現行プロファイルのマーク）
  - **Restart - in-place** — `POST /restart` で conf を変えずに再起動
  - **Restart - switch profile** — `POST /restart` で `conf` を指定してプロファイル切替再起動
- `RunWith/`（Phase VI-γ-5a）
  - **RunWith list** — `GET /run_with`（現 conf の `run_with` 配列を DTO 化）
  - **RunWith add** — `POST /run_with`（末尾追加。toml_edit でコメント温存、バックアップ付き）
  - **RunWith delete** — `DELETE /run_with/:index`（指定 index を削除）
- `Profiles/`（Phase VI-γ-4a）
  - **Profile content (read)** — `GET /profiles/:filename/content`（プロファイルの中身 TOML を取得）
  - **Profile content (update)** — `PUT /profiles/:filename/content`（TOML 書き換え + 自動バックアップ + パース検証）
  - **Profile clone** — `POST /profiles/clone`（別名で複製）
  - **Profile rename** — `POST /profiles/:filename/rename`（リネーム。現 conf は拒否）
  - **Profile delete** — `DELETE /profiles/:filename`（削除。実体は `.bak-YYYYMMDD-HHmmSS` に退避）
- `BOS/`（Phase VI-γ-2a）
  - **BOS list** — `GET /bos`（`document_root` 配下の OBS 用ブラウザソース一覧、URL / カテゴリ / channel 対応有無）
- `ManagedApps/`（Phase VI-γ-2b）
  - **ManagedApps list** — `GET /managed_apps`（`run_with` entry の spec + 現在 running/pids）
  - **ManagedApps - start** — `POST /managed_apps/:id/start`（既に running なら 409）
  - **ManagedApps - stop** — `POST /managed_apps/:id/stop`（WM_CLOSE → grace → TerminateProcess、Windows 専用）
  - **ManagedApps - minimize** — `POST /managed_apps/:id/minimize`（Windows 専用）
- `ModifyEntries/`（Phase VI-γ-8a）
  - **Dictionary entry add** — `POST /modify/:id/dictionary/entries`（`writable_dictionary_file` に 1 行追加。冪等）
  - **Dictionary entry remove** — `DELETE /modify/:id/dictionary/entries`（完全一致行を削除。他ファイル由来のエントリは不変）
  - **Regex entry add** — `POST /modify/:id/regex/entries`（`writable_regex_file` に 1 行追加。pattern は `regex::Regex::new` で事前検証）
  - **Regex entry remove** — `DELETE /modify/:id/regex/entries`
- `Processors/`（Phase VI-γ-3a + γ-3b）
  - **Processor config (read)** — `GET /processors/:index/config`（`[[processors]]` entry を JSON で返す）
  - **AI persona config (read)** — `GET /ai_personas/:index/config`（`[[ai.personas]]` entry を JSON で返す）
  - **Processor config (update)** — `PUT /processors/:index/config`（entry 差し替え + 自動バックアップ + 妥当性検証）
  - **AI persona config (update)** — `PUT /ai_personas/:index/config`（同上、`doc["ai"]["personas"]` 経路）
- `Events/`
  - **Connect (wscat)** — `GET /api/v1/control/events` の **ドキュメント用ダミー**。実体は WebSocket なので、
    Bruno からの HTTP GET では接続できない。`docs` セクションに `wscat` / ブラウザ (`new WebSocket(...)`) の
    接続例と、サーバーから飛んでくる JSON フレームのフォーマット一覧が書いてある。

## 認証の扱い

VAC の既定ポリシーは:

- ループバック (`127.0.0.1` / `::1`) → **Bearer 不要**（Tauri 前提）
- 非ループバック (LAN) → **Bearer 必須**

Local 環境の `Local.bru` から叩く分にはトークン欄は空でも動きます。

WebSocket (`/api/v1/control/events`) も同じポリシーを使います。ブラウザの `new WebSocket()` から叩くときは
`Authorization` ヘッダが付けられないため、`?token=<token>` クエリ or `?access_token=<token>` クエリで
フォールバック認証できます（`wscat` は `-H` でヘッダ付与可）。

### LAN / 強めロックで叩くとき

1. VAC 側で自動生成されたトークンの場所を確認:
   - Windows: `%LOCALAPPDATA%\virtual-avatar-connect\runtime\control-token.txt`
   - Linux/macOS: `~/.local/share/virtual-avatar-connect/runtime/control-token.txt`
2. その中身（1 行）をコピー
3. Bruno 側 environment（`Local` か `LAN`）の `bearerToken` に貼り付け（secret 扱い）
4. `collection.bru` の `auth { mode: inherit }` を `mode: bearer` に変更（collection 全体に一括適用）
   - もしくはリクエスト 1 本単位で auth を `Bearer` に切り替えても OK

## 設計上のメモ

- `auth:bearer { token: {{bearerToken}} }` は collection.bru 側で定義済み。各リクエストは `auth: inherit` で受ける。
- 既定を `mode: inherit` にしてあるのは、loopback なら無認証で動くのがふつうだから。
  認証モードを collection 全体で有効化したいときだけ上で書いた通り `mode: bearer` にする。
- `.bru` は plain text なので Git にそのまま入れて OK。**secret var は値を書かない**こと
  （Bruno は secret を `.env` などに保存するよう促す）。

## 新規エンドポイント追加時

新しいリクエストを作ったら `dev/bruno/vac-control-api/<Category>/<Name>.bru` に配置してください。`meta.seq` は
フォルダ内の相対順で、UI の並び替え用に使われます。
