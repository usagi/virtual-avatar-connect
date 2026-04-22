# `conf.toml` リファレンス (v2)

v2 配布物に含まれる `conf.toml` の全キー一覧。個別の外部サービス設定（Twitch / OpenAI / TTS / 翻訳 / OCR / Voice / 起動時 run_with / Control API 認証）の詳細は `conf.example-*.toml` を参照してください。

> **大原則**: v2 の `conf.toml` に `[[processors]]` は **書けません**（voice 暫定例外のみ、詳細は `conf.example-voice.toml`）。データの配線はすべて `flowgraph_dir` 配下の `*.flowgraph.toml` で行います。

---

## 1. 実行リソース

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `workers` | integer | CPU コア数 | 同時並行で走らせる Tokio ワーカー数。4〜8 が現実的 |
| `log_level` | string | `"Info"` | `"Error"` / `"Warn"` / `"Info"` / `"Debug"` / `"Trace"` / `"Off"` |

## 2. Web UI / Control Panel

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `web_ui_address` | string | `"127.0.0.1:57000"` | 内蔵 HTTP サーバの bind address。LAN 公開は `"0.0.0.0:57000"` 等 |
| `web_ui_compress` | bool | `true` | Actix のレスポンス圧縮。無効化したい場合のみ `false` |
| `web_ui_resources_path` | string | `"resources"` | `/resources/*` で配信する静的ファイル置き場 |
| `gui_dist_path` | string | `"gui/dist"` | GUI (Svelte) のビルド成果物。無ければ案内 HTML を返す |

## 3. Flowgraph

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `flowgraph_dir` | string | `"flowgraph"` | `*.flowgraph.toml` を再帰的に走査するルート。存在しなくても起動継続 |

## 4. 永続化 / 添付

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `state_data_path` | string | なし | ChannelDatum 配列を RON で保存するパス。未指定なら永続化なし |
| `state_data_auto_save` | bool | `true` | 入力のたびに自動保存。`false` で `/save-state` 手動時のみ |
| `state_data_pretty` | bool | `false` | 保存 RON を整形 |
| `state_data_capacity` | integer | `256` | 保持する ChannelDatum の上限件数 |
| `attachment_inline_max_bytes` | integer | `32768` | `ChannelDatum.attachments` の Inline 添付最大バイト。超過は File 降格 |
| `runtime_dir` | string | OS 既定 | ランタイム一時ディレクトリ。Windows: `%LOCALAPPDATA%\virtual-avatar-connect\runtime` |

## 5. Control API 認証（`[control_api]`）

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `bearer_token` | string | 自動生成 | 明示する場合のみ。未指定時は起動ごとに生成し `runtime_dir/control-token.txt` に書き出し |
| `require_token_for_loopback` | bool | `false` | ループバック (127.0.0.1/::1) に Bearer 必須化 |
| `require_token_for_non_loopback` | bool | `true` | LAN / 外部接続に Bearer 必須 |

※ 環境変数 `VAC_CONTROL_API_BEARER_TOKEN` でもトークンを渡せる（`conf.toml` より優先）。

## 6. 外部サービス接続（セクション単位）

以下は「外部サービスへの接続情報」であり、**データ配線は Flowgraph で行う**のが v2 の原則です。

### 6.1 `[twitch]`

Twitch OAuth / EventSub / チャット取り込み / モデレーション設定。詳細は [`conf.example-twitch.toml`](../../conf.example-twitch.toml)。

主要キー:

- `client_id`, `client_secret`（環境変数 `VAC_TWITCH_CLIENT_ID` / `VAC_TWITCH_CLIENT_SECRET` 可）
- `broadcaster_login`：配信者の Twitch login（小文字）
- `[twitch.eventsub]` / `[twitch.moderator]`：EventSub / 別トークン（モデレーター権限）

### 6.2 `[[ai.personas]]`（OpenAI Chat）

AI Persona を 1 個以上定義する配列。詳細は [`conf.example-openai-chat.toml`](../../conf.example-openai-chat.toml)。

主要キー:

- `id`, `api_key`（環境変数 `VAC_OPENAI_API_KEY` 可）, `model`
- `observe.triggers`（購読するチャンネル名）, `channel_utterance`（発話を書き出すチャンネル名）
- `system_instructions` / `custom_instructions`
- `[ai.personas.memory]` / `[ai.personas.decision]` / `[ai.personas.function_calling]`

### 6.3 TTS

TTS 関連は **conf.toml に何も書く必要がない**。すべて `flowgraph.tts.speak` ノードの入力で決める。
詳細は [`conf.example-tts.toml`](../../conf.example-tts.toml)。

### 6.4 OCR / Screenshot（Windows）

基本 conf.toml に設定不要。詳細は [`conf.example-ss-ocr.toml`](../../conf.example-ss-ocr.toml)。

### 6.5 Translation（GAS / LibreTranslate）

基本 conf.toml に設定不要。`[libretranslate]` に auto-embed 用の設定のみ任意で。詳細は [`conf.example-translate.toml`](../../conf.example-translate.toml)。

### 6.6 Voice 認識（Vosk / Whisper）

暫定的に `[[processors]] feature = "voice"` を記述する必要あり（δ-9 で `[voice]` サービスセクションに移行予定）。詳細は [`conf.example-voice.toml`](../../conf.example-voice.toml)。

### 6.7 Browser Source（`[browser_source]`）

OBS Browser Source 用 HTML のルート指定。

```toml
[browser_source]
document_root = "output"   # 既定: "output"。/output/* と /browser-output/* が参照
```

### 6.8 `run_with`

VAC 起動時に同時起動したい外部プロセスの配列。詳細は [`conf.example--run-with.toml`](../../conf.example--run-with.toml)。

---

## 7. 設定の検証

起動時に型チェックされ、エラー時は stderr と GUI の Diagnostics に出ます。よくあるエラー:

| エラー | 原因 |
|---|---|
| `[[processors]] は v2 では禁止` | conf.toml に `[[processors]]` が残っている（voice 以外は v2 では不可）|
| `flowgraph_dir が見つからない` | パスの typo。存在しなくても起動は通るので通常は警告のみ |
| `control_api token が 64 文字未満` | 手動指定時の長さチェック。自動生成なら問題なし |

より詳細な設定リファレンスは各 `conf.example-*.toml` 内のコメントが正本です。
