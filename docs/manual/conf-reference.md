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

## 3.1 `[motion]` — VMC 生 UDP パススルー（Phase M0）

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `[motion]` | テーブル | なし | 省略時は motion ワーカーなし |
| `[[motion.vmc_passthrough]]` | テーブル配列 | なし | 受信 UDP 1 ソケットごとに 1 行 |
| `enabled` | bool | `true` | `false` で当該エントリのみ無効 |
| `bind` | string | （必須） | 受信 `"host:port"`（例 `0.0.0.0:39539`） |
| `forward_to` | string 配列 | `[]` | 転送先。空なら当該エントリはスキップ（警告） |
| `label` | string | なし | ログ識別子。省略・空なら `bind` が文脈として使われる（複数受信口のハブ運用向け） |

仕様の正本: [`../roadmap/phase-mu-vmc-motion-m0.md`](../roadmap/phase-mu-vmc-motion-m0.md)（Phase M2 は同 doc §9）。例: リポジトリ直下の [`conf.example-motion.toml`](../../conf.example-motion.toml)。

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

### 5.1 `[[control_api.tables]]` — 辞書 / 汎用 Table の GUI 編集許可リスト (Phase φ)

GUI の **Dictionary Editor Pane** と **Live Quick-Add Widget** から操作できる TSV ファイルの allow-list。
`[[control_api.tables]]` に登録されていないファイルは Control API から **404**（存在隠蔽）として扱われ、
GUI のカタログにも出ません。

```toml
# 辞書を 1 件、GUI から編集 + Quick-Add トリガ可能にする例。
[[control_api.tables]]
key      = "chat_dict"                            # 必須。URL / localStorage のキー
path     = "dictionary.chat.dict.tsv"             # 必須。cwd 相対 or 絶対パス
label    = "Chat 辞書"                            # 任意。GUI 表示名。省略時は key
role     = "dictionary"                           # 任意。"dictionary" | "generic" など
editable = true                                   # 任意。false で read-only（全 mutation が 403）

[control_api.tables.quick_add]
node_id        = "chat-echo/main::learn"          # 必須。dictionary.learn ノードの fq ID
kind           = "literal"                        # 任意。"literal" | "regex"（既定 literal）
forget_node_id = "chat-echo/main::forget"         # 任意。Undo に使う dictionary.forget ノード
```

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `key` | string | — | 必須。URL (`/api/v1/control/table/{key}`) と GUI 内の識別子 |
| `path` | string | — | 必須。TSV の実パス。親ディレクトリは atomic rename 用に自動生成 |
| `label` | string | `key` | GUI タブに表示するラベル |
| `role` | string | `null` | `"dictionary"` を指定した Table だけが Dictionary Editor のタブに並ぶ |
| `editable` | bool | `true` | `false` で書き込み系 API を 403 に固定。閲覧のみ許可したい時に使う |
| `quick_add.node_id` | string | — | Live Quick-Add で発火する `flowgraph.dictionary.learn` ノードの fq ID |
| `quick_add.kind` | string | `"literal"` | 既定 kind。GUI で上書き可能 |
| `quick_add.forget_node_id` | string | `null` | Undo 用 `flowgraph.dictionary.forget` ノードの fq ID。未指定なら [Undo] を自動的に無効化 |

#### 挙動メモ

- **optimistic lock**: mutation 系（PUT / POST / PATCH / DELETE）は `If-Match: b3:<content_hash>`
  を付与すると楽観ロックが効く。GUI は常に直近ハッシュを送るため、多タブ同時編集は 409
  → 3-way 解決ダイアログで明示的にマージさせる。
- **is_locked 保護**: `is_locked = true` の行は PATCH / DELETE が 403。「Arknights 固定辞書」など
  消えてほしくない seed データを保護する用途。
- **allow-list の反映**: 現在は起動時 snapshot。`[[control_api.tables]]` を増減した場合は VAC 再起動が必要。
- **Flowgraph 側との関係**: Quick-Add / Editor が書き戻すのは **ファイル**（TSV）。
  flowgraph は `table.load_tsv` が都度読み直す実装のため、次の実行タイミングで自然に反映される。

#### 対応 Flowgraph ノード（`control_triggerable = true`）

- `flowgraph.dictionary.learn` — Quick-Add の Learn ボタン
- `flowgraph.dictionary.forget` — Quick-Add の Undo ボタン、Editor の削除

上記以外のノードに trigger API を打つと **400 control_triggerable_forbidden** が返る（安全装置）。

---

## 6. 外部サービス接続（セクション単位）

以下は「外部サービスへの接続情報」であり、**データ配線は Flowgraph で行う**のが v2 の原則です。

### 6.1 `[twitch]`

Twitch OAuth / EventSub / チャット取り込み / モデレーション設定。詳細は [`conf.example-twitch.toml`](../../conf.example-twitch.toml)。

主要キー:

- `client_id`, `client_secret`（環境変数 `VAC_TWITCH_CLIENT_ID` / `VAC_TWITCH_CLIENT_SECRET` 可）
- `broadcaster_login`：配信者の Twitch login（小文字）
- `[twitch.eventsub]` / `[twitch.moderator]`：EventSub / 別トークン（モデレーター権限）

### 6.2 `[[ai.personas]]`（OpenAI Responses API）

AI Persona を 1 個以上定義する配列。Phase χ 以降は **OpenAI Responses API (`/v1/responses`)** を唯一の経路として使う（Chat Completions は撤去済み）。詳細は [`conf.example-openai-chat.toml`](../../conf.example-openai-chat.toml)。

主要キー:

- `id`, `api_key`（環境変数 `VAC_OPENAI_API_KEY` 可）, `model`
- `observe.triggers`（購読するチャンネル名）, `channel_utterance`（発話を書き出すチャンネル名）
- `custom_instructions` / `system_instructions_extra`
- `openai_max_output_tokens`（u32, Responses API `max_output_tokens`）／ 環境変数 `VAC_OPENAI_MAX_OUTPUT_TOKENS` で上書き可
- `openai_reasoning_effort`（`"low"` / `"medium"` / `"high"`、gpt-5 系のみ有効）
- `openai_store`（bool、既定 `false`。OpenAI 側に会話 state を保存するか。VAC は client 側で memory window を完全管理するため通常 `false` のままで良い）
- `openai_reasoning_encrypted_passthrough`（bool、既定 `true`）— **gpt-5 系の tool loop round 間で `reasoning.encrypted_content` を client 側で持ち回る**（Phase ψ-α）。`store: false` を維持したまま reasoning state を transit することで、複数ラウンドに渡る tool loop の thought continuity を保つ。非 gpt-5 モデル（`gpt-4o-mini` 等）では完全 no-op（request も log も無変化）。デバッグや実機計測の比較用に明示 `false` で opt-out 可能。詳細: [`docs/roadmap/phase-psi-alpha-encrypted-reasoning.md`](../roadmap/phase-psi-alpha-encrypted-reasoning.md)
- `[ai.personas.memory]` / `[ai.personas.decision]` / `[ai.personas.function_calling]`

**互換性メモ**:

- legacy `max_tokens` (u16) は `openai_max_output_tokens` が未指定のときだけ fallback として使われる。両方指定時は新キーが優先。
- `memory_overflow_summary_max_output_tokens` (u32) が推奨。legacy `memory_overflow_summary_max_completion_tokens` (u16) は fallback。
- `openai_tools_json_path` は Responses API 形式 (`{"type":"function","name":...}`) と旧 Chat Completions 形式 (`{"type":"function","function":{...}}`) の双方を自動判別するので、既存 conf はそのまま動く。tools は **`openai_tools_json_path` に外部 JSON ファイルのパスを指定する方式のみサポート**（TOML 内に `openai_tools_json = """[...]"""` を inline で書いても黙って無視される）。

**環境変数オーバーライド**:

| 変数名 | 上書き対象 | 備考 |
|---|---|---|
| `VAC_OPENAI_API_KEY` | `api_key` | API キーは conf 直書きよりこちらを推奨 |
| `VAC_OPENAI_MAX_OUTPUT_TOKENS` | `openai_max_output_tokens` | env > 新キー > legacy `max_tokens` の順で解決 |

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
