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
| `gui_dist_path` | string | `"gui/dist"` | GUI (Svelte) のビルド成果物。無ければ案内 HTML を返す。**`cargo build --features embed-gui`** ではワークスペースの **`vac-gui-assets`** クレートが `gui/dist` を同梱するため、このキーは `/gui/*` 配信には使われない（Control API 等は従来どおり） |

## 3. Flowgraph

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `flowgraph_dir` | string | `"flowgraph"` | `*.flowgraph.toml` を再帰的に走査するルート。存在しなくても起動継続 |

### `[flowgraph]` テーブル（instance スコープ）

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `default_timezone` | string | なし | naive datetime 解釈の既定 TZ（`+09:00` 等）。未指定は UTC。詳細は Flowgraph 設定型の doc |
| `runtime_mode_changed_trigger_node_id` | string | なし | **実効 Runtime Mode が変わった直後**（noop でない遷移のあと）に、ロード済みグラフの **ノード fq ID** へ内部 `TriggerEvent` を 1 発送る。`flowgraph.ingress.web_input` 等、`__trigger__` を持つ ingress を想定。`__source_kind__` は `runtime_mode_changed` |

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

## 3.2 `[modes.*]` — Runtime Mode 宣言 (RM-1)

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `[modes.<id>]` | テーブル | なし | `<id>` はモード識別子（例 `daily`, `streaming`）。省略時は `modes` なしとして扱われる |
| `default_runtime_mode` | string | なし | 起動直後に選ぶ mode ID。指定時は `[modes.<同一 id>]` が必須 |
| `display_name` | string | なし | GUI 表示用の人間向けラベル |
| `flowgraph_groups.enable` | string 配列 | `[]` | 有効化したい Flowgraph グループ名 |
| `flowgraph_groups.disable` | string 配列 | `[]` | 無効化したいグループ名（同一モード内で enable と重複不可） |
| `managed_apps.start` / `stop` / `minimize` / `leave` | string 配列 | `[]` | `run_with` から解決される Managed App ID（明示 `id` または `run-with-<n>`） |
| `capability_policy.allow` | string 配列 | `[]` | この mode で許可したい capability。空なら allow-list 未指定として扱う。初期実装では preview のみ |
| `capability_policy.deny` | string 配列 | `[]` | この mode で抑止したい capability。`allow` との重複は validation error。初期実装では preview のみ |

ロード時に `managed_apps.*` の各 ID が `run_with` と整合するか検証される。
`capability_policy` は現時点では実行拒否ではなく、`/modes/plan` / `/modes/transit` の preview と GUI 表示で、ロード済み Flowgraph の required capability との衝突を確認するための metadata。
意味論・将来の Mode Manager との関係は [`../roadmap/runtime-mode-roadmap.md`](../roadmap/runtime-mode-roadmap.md) を参照。

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

#### Runtime Mode API（RM-3 / RM-5）

- `GET /api/v1/control/modes` — 応答 `mode_ids: string[]`（`[modes.*]` のキー一覧）。
- `GET /api/v1/control/modes/current` — 応答 `mode: string | null`（`null` は `default_runtime_mode` に従うことを意味する）。`managed_apps` は常に省略。
- `PUT /api/v1/control/modes/current` — 本文 JSON `{"mode": "..."}` または `{"mode": null}`。既知の mode 以外は 400。成功時に Flowgraph exec ゲートを再計算し、変化があれば WebSocket `runtime_mode_changed` を送る。**非 noop** のときは `[modes.*].managed_apps` を適用し、操作ログを応答の `managed_apps`（任意）と WS `runtime_mode_managed_apps` で返す。別遷移が走っているときは **409** `transition_busy`。
- `POST /api/v1/control/modes/plan` — 本文 `{"target": "..."}` または `{"target": null}`。遷移プレビュー（`ModeTransitionPlan`）。`modes` があるとき未知の `target` は 400。`capability_denied_by_target` / `capability_unlisted_by_target` は preview 専用で、現段階では実行拒否しない。
- `POST /api/v1/control/modes/transit` — 本文 `{"mode": "...", "dry_run": false, "reason": "..."}`。`dry_run: true` のときは状態を変えず `plan` のみ返す。`dry_run: false` で `PUT .../current` と同様の適用＋応答に `plan` を含む。**非 noop** 時は `managed_apps` 配列を任意同梱。再入時は **409**。

#### Flowgraph Diagnostics API（LF-7）

`GET /api/v1/control/flowgraph/diagnostics` は、ロード診断に加えて Flowgraph の signature / capability / state metadata を返します。
`GET /api/v1/control/flowgraph/signature` は、同じ `graph_signature` JSON だけを返します。
`GET /api/v1/control/flowgraph/package-lock` は、flowgraph root 直下の `flowgraph.lock.json` を読み、保存済み digest と現在の preview digest が一致するかを返します。
`GET /api/v1/control/flowgraph/package-lock-preview` は、`digest`, `entries`, `entry_count` だけを返す read-only package lock preview API です。
`POST /api/v1/control/flowgraph/package-lock-preview/save` は、現在の package lock preview を flowgraph root 直下の `flowgraph.lock.json` に保存します。
package lockfile の JSON envelope は `kind = "vac.flowgraph.package_lock"`, `schema_version = 1`, `digest`, `entry_count`, `entries` を持ち、既定ファイル名は `flowgraph.lock.json` です。
`package_manifests` は top-level `[package]` を file ごとに集約した catalog です。`package_dependency_order` は local package dependency DAG の dependency-first order です。`package_lock_preview` は lockfile 生成前に diagnostics API から観測できる read-only preview で、各 entry は raw `.flowgraph.toml` content の `source_digest` と lock entry 全体の `digest` を `b3:<64 hex>` 形式で持ちます。`package_lock_preview_digest` は preview 全体の `b3:<64 hex>` digest です。
`*.flowgraph.toml` は任意の top-level `[package]` を読めます。現在のフィールドは `id`, `version`, `exports` で、`graph_signature.files[]` の `package_id`, `package_version`, `package_exports` として公開されます。
`[package].id` は必須で、`example.pkg-name` のような lowercase dot-separated identifier です。同一 flowgraph root 内で package id は重複できません。`[package].version` は任意ですが、書く場合は SemVer 形式です。`[package].exports` は flowgraph ルート内に存在する fq を指定します。`[package.dependencies]` は dependency package id から version requirement への table で、現在は exact SemVer または `"*"` を読めます。package id は dot を含むため `"example.dep" = "1.2.3"` のように quoted key で書きます。dependency id は同一 flowgraph root 内の `[package].id` と一致する必要があります。exact SemVer は dependency package の `[package].version` と一致した場合だけ有効で、`"*"` は version 未指定の dependency も許可します。空エントリ、未知 fq、正規化後に重複する fq、不正 dependency、未存在 dependency、version mismatch、自分自身への dependency、dependency cycle は `invalid-package-manifest` としてロードエラーになります。
state 関連の値は reload 直後の read-only snapshot であり、worker 実行後の live state ではありません。

- `graph_signature`: graph-as-node / library signature の入口となる read-only boundary metadata。
- `graph_signature.files[]`: Flowgraph file fq、title / description、normalized library id、mode activation metadata。
- `graph_signature.external_triggers[]`: `flowgraph.ingress.*` と control-triggerable node の外部 trigger surface。
- `graph_signature.boundary_inputs[]` / `boundary_outputs[]`: edge で内部接続されていない port surface。
- `capability_summary.stateful_node_count`: graph 内の stateful node 数。
- `capability_summary.snapshot_supported_state_node_count`: graph 内で snapshot export に対応する stateful node 数。
- `capability_summary.restore_supported_state_node_count`: graph 内で snapshot restore に対応する stateful node 数。
- `capability_summary.state_nodes[]`: node ごとの scope / storage / lifetime / snapshot / restore / persistence policy。
- `loaded_state_summary`: ロード直後の stateful node、feature、version、state model の一覧。
- `loaded_state_summary.snapshot_supported_node_count`: ロード直後 program の snapshot 対応 node 数。
- `loaded_state_summary.restore_supported_node_count`: ロード直後 program の restore 対応 node 数。
- `loaded_state_snapshot`: ロード直後に export できた snapshot payload。現段階では対応 node のみ含む。

現在は `flowgraph.state.bool`、`flowgraph.state.int_counter`、`flowgraph.state.latch`、`flowgraph.state.accumulator`、`flowgraph.util.rate_limit` が JSON snapshot / restore に対応しています。
内部の state snapshot ファイル形式は `kind = "vac.flowgraph.state_snapshot"`、`schema_version = 1` の JSON envelope として固定しています。
profile-local snapshot の保存先は `<runtime_dir>/flowgraph-state/<profile-stem>-<hash>/state.snapshot.json` 形式で導出します。
runtime loader には明示的な load-time restore helper があり、raw snapshot payload または state snapshot file envelope から復元できます。
worker spawn 経路にも snapshot file envelope を明示指定する内部 helper があり、復元後の state を持つ program をそのまま起動できます。
明示 restore 付き load / spawn の結果は `loaded_state_restore_report` として diagnostics に残り、restore された node 数を確認できます。
`GET /flowgraph/diagnostics` は `state_snapshot_file_path` で現在の profile / flowgraph root に対応する予定保存先を返します。
同じ応答の `state_snapshot_file_exists` は、その path に snapshot file envelope が存在するかを返します。
runtime には `loaded_state_snapshot` をその予定保存先へ `ProgramStateSnapshotFile` envelope として明示 write する内部 helper があります。
`POST /flowgraph/state-snapshot/loaded/save` は同じ helper を呼び、worker 実行後の live state ではなく `loaded_state_snapshot` を明示保存します。
`POST /flowgraph/state-snapshot/live/save` は起動中 worker から現在の live state snapshot を取得し、同じ profile-local snapshot file path へ明示保存します。
`POST /flowgraph/reload/preserve-state` は live state snapshot を同 path へ保存してから、その file envelope を使って reload / restore します。
GUI の Flowgraph Diagnostics でも `state_snapshot_file_path` がある場合に loaded snapshot 保存用の `save loaded`、live snapshot 保存用の `save live`、live state 保持 reload 用の `reload keep state` を表示します。
Flowgraph Studio の toolbar / command palette からも通常 reload とは別に live state 保持 reload を明示実行できます。
`POST /flowgraph/state-snapshot/profile-local/restore` は `state_snapshot_file_path` の JSON envelope を明示 restore して Flowgraph runtime を reload / respawn します。
GUI の Flowgraph Diagnostics でも同じ path 行に `restore snapshot` 操作を表示し、restore 後に diagnostics を再取得します。
restore や snapshot file 読込に失敗した場合は `state-restore` diagnostic として報告されますが、profile-local persistence、migration、通常 reload 時の自動 restore はまだ導入していません。

### 5.1 `[[control_api.tables]]` — Glossary / 汎用 Table の GUI 編集許可リスト (Phase φ / GRN)

GUI の **Glossary Editor Pane** と **Live Quick-Add Widget** から操作できる TSV ファイルの allow-list。
`[[control_api.tables]]` に登録されていないファイルは Control API から **404**（存在隠蔽）として扱われ、
GUI のカタログにも出ません。

```toml
# Glossary を 1 件、GUI から編集 + Quick-Add トリガ可能にする例。
[[control_api.tables]]
key      = "chat_dict"                            # 必須。URL / localStorage のキー
path     = "dictionary.chat.dict.tsv"             # 必須。cwd 相対 or 絶対パス
label    = "Chat 用語集"                          # 任意。GUI 表示名。省略時は key
role     = "glossary"                             # 任意。"glossary" | "generic" など
editable = true                                   # 任意。false で read-only（全 mutation が 403）

[control_api.tables.quick_add]
node_id        = "chat-echo/main::learn"          # 必須。glossary.learn ノードの fq ID
kind           = "literal"                        # 任意。"literal" | "regex"（既定 literal）
forget_node_id = "chat-echo/main::forget"         # 任意。Undo に使う glossary.forget ノード
```

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `key` | string | — | 必須。URL (`/api/v1/control/table/{key}`) と GUI 内の識別子 |
| `path` | string | — | 必須。TSV の実パス。親ディレクトリは atomic rename 用に自動生成 |
| `label` | string | `key` | GUI タブに表示するラベル |
| `role` | string | `null` | `"glossary"` を指定した Table だけが Glossary Editor のタブに並ぶ |
| `editable` | bool | `true` | `false` で書き込み系 API を 403 に固定。閲覧のみ許可したい時に使う |
| `quick_add.node_id` | string | — | Live Quick-Add で発火する `flowgraph.glossary.learn` ノードの fq ID |
| `quick_add.kind` | string | `"literal"` | 既定 kind。GUI で上書き可能 |
| `quick_add.forget_node_id` | string | `null` | Undo 用 `flowgraph.glossary.forget` ノードの fq ID。未指定なら [Undo] を自動的に無効化 |

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

- `flowgraph.glossary.learn` — Quick-Add の Learn ボタン
- `flowgraph.glossary.forget` — Quick-Add の Undo ボタン、Editor の削除

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
