# Flowgraph Node Catalog

VAC v2 Flowgraph の組み込みノード一覧。**本ファイルは自動生成**されるので、手で編集せずノード定義側を更新してからテストで再生成してください。

再生成:

```powershell
$env:BLESS_NODE_CATALOG="1"; cargo test --lib node_catalog_md_up_to_date
```

> 型の表記: `bool` / `int` / `float` / `string` / `json` / `list<T>` / `map<T>` / `exec`

## Index

- **channel**
  - [`flowgraph.channel.emit`](#flowgraph-channel-emit) — Channel Emit
- **command**
  - [`flowgraph.command.match`](#flowgraph-command-match) — Command Match
  - [`flowgraph.command.set`](#flowgraph-command-set) — Command Set
- **compare**
  - [`flowgraph.compare.eq`](#flowgraph-compare-eq) — Equal
  - [`flowgraph.compare.float_gt`](#flowgraph-compare-float-gt) — Float >
  - [`flowgraph.compare.float_lt`](#flowgraph-compare-float-lt) — Float <
  - [`flowgraph.compare.int_ge`](#flowgraph-compare-int-ge) — Int >=
  - [`flowgraph.compare.int_gt`](#flowgraph-compare-int-gt) — Int >
  - [`flowgraph.compare.int_le`](#flowgraph-compare-int-le) — Int <=
  - [`flowgraph.compare.int_lt`](#flowgraph-compare-int-lt) — Int <
  - [`flowgraph.compare.neq`](#flowgraph-compare-neq) — Not Equal
- **convert**
  - [`flowgraph.convert.float_to_int`](#flowgraph-convert-float-to-int) — Float → Int
  - [`flowgraph.convert.float_to_string`](#flowgraph-convert-float-to-string) — Float → String
  - [`flowgraph.convert.int_to_float`](#flowgraph-convert-int-to-float) — Int → Float
  - [`flowgraph.convert.int_to_string`](#flowgraph-convert-int-to-string) — Int → String
  - [`flowgraph.convert.string_to_float`](#flowgraph-convert-string-to-float) — String → Float
  - [`flowgraph.convert.string_to_int`](#flowgraph-convert-string-to-int) — String → Int
- **dictionary**
  - [`flowgraph.dictionary.forget`](#flowgraph-dictionary-forget) — Dictionary Forget
  - [`flowgraph.dictionary.learn`](#flowgraph-dictionary-learn) — Dictionary Learn
  - [`flowgraph.dictionary.match`](#flowgraph-dictionary-match) — Dictionary Match
  - [`flowgraph.dictionary.replace`](#flowgraph-dictionary-replace) — Dictionary Replace
- **flow**
  - [`flowgraph.flow.branch`](#flowgraph-flow-branch) — Branch
  - [`flowgraph.flow.gate`](#flowgraph-flow-gate) — Gate
- **ingress**
  - [`flowgraph.ingress.channel_subscribe`](#flowgraph-ingress-channel-subscribe) — Channel Subscribe Ingress
  - [`flowgraph.ingress.twitch`](#flowgraph-ingress-twitch) — Twitch Ingress
  - [`flowgraph.ingress.twitch_eventsub`](#flowgraph-ingress-twitch-eventsub) — Twitch EventSub Ingress
  - [`flowgraph.ingress.voice`](#flowgraph-ingress-voice) — Voice Ingress
  - [`flowgraph.ingress.web_input`](#flowgraph-ingress-web-input) — Web Input Ingress
- **json**
  - [`flowgraph.json.get`](#flowgraph-json-get) — JSON Get
  - [`flowgraph.json.parse`](#flowgraph-json-parse) — JSON Parse
  - [`flowgraph.json.stringify`](#flowgraph-json-stringify) — JSON Stringify
- **list**
  - [`flowgraph.list.get`](#flowgraph-list-get) — List Get
  - [`flowgraph.list.is_empty`](#flowgraph-list-is-empty) — List Is Empty
  - [`flowgraph.list.len`](#flowgraph-list-len) — List Length
- **literal**
  - [`flowgraph.literal.bool`](#flowgraph-literal-bool) — Bool Literal
  - [`flowgraph.literal.float`](#flowgraph-literal-float) — Float Literal
  - [`flowgraph.literal.int`](#flowgraph-literal-int) — Int Literal
  - [`flowgraph.literal.json`](#flowgraph-literal-json) — JSON Literal
  - [`flowgraph.literal.string`](#flowgraph-literal-string) — String Literal
- **logic**
  - [`flowgraph.logic.and`](#flowgraph-logic-and) — And
  - [`flowgraph.logic.not`](#flowgraph-logic-not) — Not
  - [`flowgraph.logic.or`](#flowgraph-logic-or) — Or
  - [`flowgraph.logic.xor`](#flowgraph-logic-xor) — Xor
- **map**
  - [`flowgraph.map.get`](#flowgraph-map-get) — Map Get
  - [`flowgraph.map.has`](#flowgraph-map-has) — Map Has
  - [`flowgraph.map.keys`](#flowgraph-map-keys) — Map Keys
- **math**
  - [`flowgraph.math.float_add`](#flowgraph-math-float-add) — Float +
  - [`flowgraph.math.float_div`](#flowgraph-math-float-div) — Float /
  - [`flowgraph.math.float_mul`](#flowgraph-math-float-mul) — Float *
  - [`flowgraph.math.float_sub`](#flowgraph-math-float-sub) — Float -
  - [`flowgraph.math.int_add`](#flowgraph-math-int-add) — Int +
  - [`flowgraph.math.int_div`](#flowgraph-math-int-div) — Int /
  - [`flowgraph.math.int_mod`](#flowgraph-math-int-mod) — Int %
  - [`flowgraph.math.int_mul`](#flowgraph-math-int-mul) — Int *
  - [`flowgraph.math.int_sub`](#flowgraph-math-int-sub) — Int -
- **ocr**
  - [`flowgraph.ocr.recognize`](#flowgraph-ocr-recognize) — OCR Recognize
- **regex**
  - [`flowgraph.regex.replace`](#flowgraph-regex-replace) — Regex Replace
- **screenshot**
  - [`flowgraph.screenshot.capture`](#flowgraph-screenshot-capture) — Screenshot Capture
- **state**
  - [`flowgraph.state.accumulator`](#flowgraph-state-accumulator) — Accumulator
  - [`flowgraph.state.bool`](#flowgraph-state-bool) — Bool State
  - [`flowgraph.state.int_counter`](#flowgraph-state-int-counter) — Int Counter
  - [`flowgraph.state.latch`](#flowgraph-state-latch) — Latch
- **string**
  - [`flowgraph.string.concat`](#flowgraph-string-concat) — String Concat
  - [`flowgraph.string.contains`](#flowgraph-string-contains) — String Contains
  - [`flowgraph.string.join`](#flowgraph-string-join) — String Join
  - [`flowgraph.string.len`](#flowgraph-string-len) — String Length
  - [`flowgraph.string.replace`](#flowgraph-string-replace) — String Replace
  - [`flowgraph.string.split`](#flowgraph-string-split) — String Split
- **table**
  - [`flowgraph.table.from_json`](#flowgraph-table-from-json) — Table From JSON
  - [`flowgraph.table.load_tsv`](#flowgraph-table-load-tsv) — Table Load TSV
  - [`flowgraph.table.to_json`](#flowgraph-table-to-json) — Table To JSON
  - [`flowgraph.table.write_tsv`](#flowgraph-table-write-tsv) — Table Write TSV
- **translate**
  - [`flowgraph.translate.gas`](#flowgraph-translate-gas) — Translate (GAS)
  - [`flowgraph.translate.libre`](#flowgraph-translate-libre) — Translate (LibreTranslate)
- **tts**
  - [`flowgraph.tts.speak`](#flowgraph-tts-speak) — TTS: Speak
- **twitch**
  - [`flowgraph.twitch.ban`](#flowgraph-twitch-ban) — Twitch: Ban User
  - [`flowgraph.twitch.chat_send`](#flowgraph-twitch-chat-send) — Twitch: Chat Send
  - [`flowgraph.twitch.get_token`](#flowgraph-twitch-get-token) — Twitch: Get Token
  - [`flowgraph.twitch.timeout`](#flowgraph-twitch-timeout) — Twitch: Timeout User
  - [`flowgraph.twitch.user_id_by_login`](#flowgraph-twitch-user-id-by-login) — Twitch: User ID by Login
  - [`flowgraph.twitch.validate_token`](#flowgraph-twitch-validate-token) — Twitch: Validate Token
- **unit**
  - [`flowgraph.unit.assign`](#flowgraph-unit-assign) — Unit Assign
  - [`flowgraph.unit.convert`](#flowgraph-unit-convert) — Unit Convert
  - [`flowgraph.unit.get_dim_string`](#flowgraph-unit-get-dim-string) — Dimension -> String
  - [`flowgraph.unit.get_unit_string`](#flowgraph-unit-get-unit-string) — Unit -> String
  - [`flowgraph.unit.same_dimension`](#flowgraph-unit-same-dimension) — Same Dimension?
  - [`flowgraph.unit.strip`](#flowgraph-unit-strip) — Unit Strip
  - [`flowgraph.unit.to_json`](#flowgraph-unit-to-json) — Unit -> JSON
- **util**
  - [`flowgraph.util.delay`](#flowgraph-util-delay) — Delay
  - [`flowgraph.util.format`](#flowgraph-util-format) — Format Quantity
  - [`flowgraph.util.log`](#flowgraph-util-log) — Log
  - [`flowgraph.util.rate_limit`](#flowgraph-util-rate-limit) — Rate Limit

## channel

### `flowgraph.channel.emit`

**Channel Emit** — State.channel_data に ChannelDatum を push。WS クライアント / browser-output に届く終端ノード。`content` / `channel` / `source_actor` は `String`。`Quantity` を配線した場合は engine 側で`"{value} {unit}"` 形式に自動文字列化される。単位を含めたくない場合は手前で`flowgraph.util.format` (include_unit=false) か `flowgraph.unit.strip` を挟む。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `channel` | `string` | `""` |  |
| `content` | `string` | `""` |  |
| `is_final` | `bool` | `true` |  |
| `source_actor` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `fallback_channel` | `string` | `""` |  | `channel` 入力が空のときに使うフォールバックチャンネル名。両方空なら on_error 発火。 |
| `auto_tag_source_actor` | `bool` | `true` |  | `source_actor` 入力が空のとき、自動で `flowgraph:<node_id>` を stamp する。`channel.subscribe` のデフォルト echo filter と協調する。false で無効化。 |

## command

### `flowgraph.command.match`

**Command Match** — prefix 付きコマンド文字列を verb + args にパースして exec を分岐

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `content` | `string` | — |  |
| `prefix` | `string` | `"/"` |  |

| Output | Type | Note |
|---|---|---|
| `on_command` | `exec` (out) |  |
| `on_other` | `exec` (out) |  |
| `command` | `string` |  |
| `args` | `list<string>` |  |
| `original` | `string` |  |

### `flowgraph.command.set`

**Command Set** — command_name に一致する set を sets プロパティから引き、pre → channel_contents → post の順でチャンネルへ push する

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `command_name` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_set` | `exec` (out) |  |
| `on_none` | `exec` (out) |  |
| `matched_name` | `string` |  |
| `entry_count` | `int` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `sets` | `json` | `[]` |  | コマンドセット配列。各要素は `{ name, pre?, post?, channel_contents? }` を持つ JSON。 |
| `pre_post_channel` | `string` | `""` |  | `pre` / `post` を流すチャンネル名。空なら pre/post を無視する。 |
| `source_actor` | `string` | `""` |  | ChannelDatum の source_actor。空なら `flowgraph:<node_id>` を自動スタンプ。 |

## compare

### `flowgraph.compare.eq`

**Equal** — Json 値同士を比較

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.compare.float_gt`

**Float >**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `float` | — |  |
| `b` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.compare.float_lt`

**Float <**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `float` | — |  |
| `b` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.compare.int_ge`

**Int >=**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.compare.int_gt`

**Int >**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.compare.int_le`

**Int <=**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.compare.int_lt`

**Int <**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.compare.neq`

**Not Equal**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

## convert

### `flowgraph.convert.float_to_int`

**Float → Int** — 切り捨て（trunc）

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.convert.float_to_string`

**Float → String**

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |

### `flowgraph.convert.int_to_float`

**Int → Float**

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.convert.int_to_string`

**Int → String**

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |

### `flowgraph.convert.string_to_float`

**String → Float** — パース失敗はエラー halt

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.convert.string_to_int`

**String → Int** — パース失敗はエラー halt

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

## dictionary

### `flowgraph.dictionary.forget`

**Dictionary Forget** — Table 辞書から source (+ replacement) 一致行を削除。mode=latest/all/exact、is_locked 保護

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `dictionary` | `table` | `[]` |  |
| `source` | `string` | — |  |
| `replacement` | `string` | `""` |  |
| `mode` | `string` | `"latest"` |  |

| Output | Type | Note |
|---|---|---|
| `on_forgotten` | `exec` (out) |  |
| `on_nothing` | `exec` (out) |  |
| `on_locked` | `exec` (out) |  |
| `updated_dictionary` | `table` |  |
| `removed_count` | `int` |  |
| `locked_count` | `int` |  |
| `feedback` | `string` |  |

### `flowgraph.dictionary.learn`

**Dictionary Learn** — Table 辞書に 11 カラムエントリを append。同値エントリは duplicate 検出して no-op

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `dictionary` | `table` | `[]` |  |
| `source` | `string` | — |  |
| `replacement` | `string` | — |  |
| `kind` | `string` | `"literal"` |  |
| `priority` | `int` | `0` |  |
| `by` | `string` | `""` |  |
| `tags` | `string` | `""` |  |
| `note` | `string` | `""` |  |
| `expires_at` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_learned` | `exec` (out) |  |
| `on_duplicate` | `exec` (out) |  |
| `updated_dictionary` | `table` |  |
| `added_entry` | `json` |  |
| `feedback` | `string` |  |

### `flowgraph.dictionary.match`

**Dictionary Match** — Table 辞書で text を照合し、一致エントリと captures を取り出す。exec 分岐可能。Stateful

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `text` | `string` | — |  |
| `dictionary` | `table` | `[]` |  |

| Output | Type | Note |
|---|---|---|
| `on_match` | `exec` (out) |  |
| `on_no_match` | `exec` (out) |  |
| `matched_entries` | `list<json>` |  |
| `matched_count` | `int` |  |
| `captures` | `list<list<string>>` |  |
| `first_replacement` | `string` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `match_policy` | `string` | `"first"` |  | first / all / longest |
| `anchor` | `string` | `"anywhere"` |  | anywhere / prefix / full |

### `flowgraph.dictionary.replace`

**Dictionary Replace** — Table 辞書（11 カラム）で content を literal(AC) + regex 統合で逐次置換。Stateful（AC/Regex キャッシュ）

| Input | Type | Default | Note |
|---|---|---|---|
| `content` | `string` | — |  |
| `dictionary` | `table` | `[]` |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |
| `applied_count` | `int` |  |

## flow

### `flowgraph.flow.branch`

**Branch** — cond が true なら then を、false なら else を発火

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `cond` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `then` | `exec` (out) |  |
| `else` | `exec` (out) |  |

### `flowgraph.flow.gate`

**Gate** — open=true の間だけ exec を通す（ステートレス）

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `open` | `bool` | `true` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |

## ingress

### `flowgraph.ingress.channel_subscribe`

**Channel Subscribe Ingress** — V1 `State.channel_data` への push を監視し、channels / is_final / source_actor フィルタを通過した ChannelDatum を TriggerEvent として流し込む。channel.emit の対称入口で、既存 Processor 時代の channel_from 駆動パイプラインを Flowgraph で再現する基盤。

| Input | Type | Default | Note |
|---|---|---|---|
| `__trigger__` | `exec` (in) | — |  |
| `__channel__` | `string` | `""` |  |
| `__content__` | `string` | `""` |  |
| `__source_actor__` | `string` | `""` |  |
| `__is_final__` | `bool` | `true` |  |
| `__meta__` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `channel` | `string` |  |
| `content` | `string` |  |
| `source_actor` | `string` |  |
| `is_final` | `bool` |  |
| `meta` | `json` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `channels` | `list<string>` | `[]` |  | 購読対象のチャンネル名リスト。空なら全チャンネルを受ける。 |
| `require_final` | `bool` | `true` |  | true なら `is_final` フラグが立っている datum のみ発火。ストリーミング中間を無視する。 |
| `ignore_source_actors` | `list<string>` | `[]` |  | meta.source_actor がこの一覧に含まれる datum は drop。自 bot 由来のエコーを遮断する用途。 |
| `ignore_flowgraph_echo` | `bool` | `true` |  | true なら meta.source_actor が `flowgraph:` / `flowgraph` で始まる datum を drop。`channel.emit` による自グラフ押し返しを防ぐ既定動作。 |
| `require_flags` | `list<string>` | `[]` |  | 指定フラグをすべて持つ datum のみ発火。例: `["is_final"]`。 |
| `drop_flags` | `list<string>` | `[]` |  | 指定フラグを 1 つでも持つ datum を drop。例: voice_vosk 由来を遮断する等。 |

### `flowgraph.ingress.twitch`

**Twitch Ingress** — Twitch チャット / EventSub 経由の入力

| Input | Type | Default | Note |
|---|---|---|---|
| `__trigger__` | `exec` (in) | — |  |
| `__content__` | `string` | `""` |  |
| `__source_actor__` | `string` | `""` |  |
| `__source_kind__` | `string` | `""` |  |
| `__meta__` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `content` | `string` |  |
| `source_actor` | `string` |  |
| `source_kind` | `string` |  |
| `meta` | `json` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `mode` | `string` | `"irc"` |  | "irc"（チャット受信）か "eventsub"（Helix/EventSub）。 |
| `channels` | `list<string>` | `[]` |  | IRC 購読するチャンネル名（`#` 不要）。 |
| `access_token` | `string` | `""` |  | OAuth トークン（空なら `token_key` による解決を試みる）。 |
| `token_key` | `string` | `""` |  | `conf.twitch.token_keys` で定義したキー。空なら `access_token` をそのまま使用。 |
| `client_id` | `string` | `""` |  | Twitch App の Client ID。EventSub で必須。 |
| `broadcaster_id` | `string` | `""` |  | EventSub subscribe 対象の broadcaster_user_id。 |
| `login` | `string` | `""` |  | IRC ログイン名（小文字・`#` 不要）。 |
| `fixed_channel` | `string` | `""` |  | ingress が echo で出す `source_kind`。空なら `twitch:chat` または `twitch:eventsub`。 |

### `flowgraph.ingress.twitch_eventsub`

**Twitch EventSub Ingress** — Twitch EventSub (WebSocket) の通知を受ける Flowgraph-native ingress。`token_key` で conf.twitch.token_keys の OAuth トークンを引き、`event_types` に挙げた sub_type を購読する。`payload` は生 event JSON、`meta` はサブスクリプション情報を含む補助マップ。

| Input | Type | Default | Note |
|---|---|---|---|
| `__trigger__` | `exec` (in) | — |  |
| `__event_type__` | `string` | `""` |  |
| `__actor_name__` | `string` | `""` |  |
| `__actor_login__` | `string` | `""` |  |
| `__broadcaster_login__` | `string` | `""` |  |
| `__payload__` | `json` | `null` |  |
| `__meta__` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `event_type` | `string` |  |
| `actor_name` | `string` |  |
| `actor_login` | `string` |  |
| `broadcaster_login` | `string` |  |
| `payload` | `json` |  |
| `meta` | `json` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `token_key` | `string` | `"broadcaster"` |  | `conf.twitch.token_keys` で定義した OAuth key。EventSub 購読には user access token が必須。 |
| `broadcaster_login` | `string` | `""` |  | 購読対象の配信者ログイン名。空なら `conf.twitch.username`、それも無ければ `token_key` の OAuth login が使われる。 |
| `event_types` | `list<string>` | `[]` |  | 購読する sub_type リスト（例: `["channel.cheer", "channel.raid"]`）。空なら conf.twitch.eventsub の 各 bool トグル（stream.online / channel.cheer ...）をフォールバックとして使う。 |
| `channel_points_reward_id` | `string` | `""` |  | `channel.channel_points_custom_reward_redemption.add` を個別 reward に絞りたい時の UUID。空なら報酬を自動列挙してすべて購読する。 |
| `fixed_channel` | `string` | `""` |  | （予約）将来 V1 channel に押し戻すブリッジ用。今はコメントのみ。 |

### `flowgraph.ingress.voice`

**Voice Ingress** — 音声認識（Vosk / Whisper）経由の入力

| Input | Type | Default | Note |
|---|---|---|---|
| `__trigger__` | `exec` (in) | — |  |
| `__content__` | `string` | `""` |  |
| `__source_actor__` | `string` | `""` |  |
| `__source_kind__` | `string` | `""` |  |
| `__meta__` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `content` | `string` |  |
| `source_actor` | `string` |  |
| `source_kind` | `string` |  |
| `meta` | `json` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `engine` | `string` | `"vosk"` |  | "vosk" か "whisper"。 |
| `model_path` | `string` | `""` |  | Vosk/Whisper のモデル格納ディレクトリ。bridge 起動時にロードする。 |
| `source` | `string` | `""` |  | 入力音声ソース（マイクデバイス名 / ファイルパス / 空＝既定デバイス）。 |
| `sample_rate` | `int` | `16000` |  | サンプリングレート（Hz）。vosk は通常 16000、whisper は任意。 |
| `language_code` | `string` | `"ja"` |  | 言語コード（whisper で使用）。例 "ja" / "en"。 |
| `grammar_json` | `string` | `""` |  | vosk の constrained grammar を JSON 文字列で指定（省略可）。 |
| `fixed_channel` | `string` | `""` |  | ingress が echo で出す `source_kind`。空なら `voice`。 |

### `flowgraph.ingress.web_input`

**Web Input Ingress** — HTTP エンドポイント経由の入力（V1 の /input/* 相当）

| Input | Type | Default | Note |
|---|---|---|---|
| `__trigger__` | `exec` (in) | — |  |
| `__content__` | `string` | `""` |  |
| `__source_actor__` | `string` | `""` |  |
| `__source_kind__` | `string` | `""` |  |
| `__meta__` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `content` | `string` |  |
| `source_actor` | `string` |  |
| `source_kind` | `string` |  |
| `meta` | `json` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `path` | `string` | `""` |  | HTTP パス（例 "/input/chat"）。空なら `/input/flowgraph/<node_id>` を自動採番。 |
| `method` | `string` | `"POST"` |  | HTTP メソッド。"POST"/"GET"/"PUT" 等（大文字小文字不問）。 |
| `body_format` | `string` | `"plain"` |  | リクエストボディの解釈。"plain"（本文をそのまま content）/"json"（`content` フィールド）/"form"（`content` パラメータ）。 |
| `fixed_channel` | `string` | `""` |  | ingress が echo で出す `source_kind`（V1 の channel 相当）。空なら `web_input`。 |

## json

### `flowgraph.json.get`

**JSON Get** — ドットパス（foo.bar[0].baz 等）で値を抜き出す。存在しない場合は Null

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `json` | — |  |
| `path` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.json.parse`

**JSON Parse** — String を JSON にパース（失敗時はエラー halt）

| Input | Type | Default | Note |
|---|---|---|---|
| `text` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `value` | `json` |  |

### `flowgraph.json.stringify`

**JSON Stringify** — Json → String（pretty=true で整形出力）

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `json` | — |  |
| `pretty` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `text` | `string` |  |

## list

### `flowgraph.list.get`

**List Get** — index が範囲外なら Json::Null

| Input | Type | Default | Note |
|---|---|---|---|
| `items` | `list<json>` | — |  |
| `index` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `item` | `json` |  |

### `flowgraph.list.is_empty`

**List Is Empty**

| Input | Type | Default | Note |
|---|---|---|---|
| `items` | `list<json>` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.list.len`

**List Length**

| Input | Type | Default | Note |
|---|---|---|---|
| `items` | `list<json>` | — |  |

| Output | Type | Note |
|---|---|---|
| `len` | `int` |  |

## literal

### `flowgraph.literal.bool`

**Bool Literal** — BoolLiteralNode literal

| Output | Type | Note |
|---|---|---|
| `value` | `bool` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `value` | `bool` | `false` |  |  |

### `flowgraph.literal.float`

**Float Literal** — FloatLiteralNode literal

| Output | Type | Note |
|---|---|---|
| `value` | `float` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `value` | `float` | `0.0` |  |  |

### `flowgraph.literal.int`

**Int Literal** — IntLiteralNode literal

| Output | Type | Note |
|---|---|---|
| `value` | `int` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `value` | `int` | `0` |  |  |

### `flowgraph.literal.json`

**JSON Literal** — JsonLiteralNode literal

| Output | Type | Note |
|---|---|---|
| `value` | `json` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `value` | `json` | `null` |  |  |

### `flowgraph.literal.string`

**String Literal** — StringLiteralNode literal

| Output | Type | Note |
|---|---|---|
| `value` | `string` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `value` | `string` | `""` |  |  |

## logic

### `flowgraph.logic.and`

**And**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `bool` | — |  |
| `b` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.logic.not`

**Not**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.logic.or`

**Or**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `bool` | — |  |
| `b` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.logic.xor`

**Xor**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `bool` | — |  |
| `b` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

## map

### `flowgraph.map.get`

**Map Get** — キーが存在しない場合は Json::Null

| Input | Type | Default | Note |
|---|---|---|---|
| `m` | `map<json>` | — |  |
| `key` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `value` | `json` |  |

### `flowgraph.map.has`

**Map Has**

| Input | Type | Default | Note |
|---|---|---|---|
| `m` | `map<json>` | — |  |
| `key` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.map.keys`

**Map Keys**

| Input | Type | Default | Note |
|---|---|---|---|
| `m` | `map<json>` | — |  |

| Output | Type | Note |
|---|---|---|
| `keys` | `list<string>` |  |

## math

### `flowgraph.math.float_add`

**Float +** — Quantity 加算。dim 不一致はエラー。ΔK + K(abs) は許容、K + K はエラー（abs 同士加算禁止）。

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.float_div`

**Float /** — Quantity 除算。dim は差分で組み立てられる（m / s = m·s⁻¹）。絶対温度の絡む除算や 0 除算はエラー。

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.float_mul`

**Float *** — Quantity 乗算。dim は組み立てられる（m * s = m·s）。絶対温度を絡めた乗算は禁止。

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.float_sub`

**Float -** — Quantity 減算。dim 不一致はエラー。K - K は ΔK を生成。

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.int_add`

**Int +**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.int_div`

**Int /**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.int_mod`

**Int %**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.int_mul`

**Int ***

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.int_sub`

**Int -**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

## ocr

### `flowgraph.ocr.recognize`

**OCR Recognize** — 画像ソースから文字列を OCR 抽出する（Windows のみ）

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `source` | `string` | — |  |
| `lang` | `string` | — |  |
| `lines` | `bool` | `false` |  |
| `check_result_lang` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `text` | `string` |  |
| `error` | `string` |  |

## regex

### `flowgraph.regex.replace`

**Regex Replace** — 正規表現ルール（{pattern, replacement}）で content を逐次置換する

| Input | Type | Default | Note |
|---|---|---|---|
| `content` | `string` | — |  |
| `rules` | `list<json>` | `[]` |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |
| `errors` | `list<string>` |  |

## screenshot

### `flowgraph.screenshot.capture`

**Screenshot Capture** — ウィンドウ/デスクトップのスクリーンショットを PNG data URL として取得

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `target_title` | `string` | `""` |  |
| `target_title_regex` | `string` | `""` |  |
| `client_only` | `bool` | `false` |  |
| `use_bitblt` | `bool` | `false` |  |
| `crop` | `json` | `null` |  |
| `save_path` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `data_url` | `string` |  |
| `saved_path` | `string` |  |
| `width` | `int` |  |
| `height` | `int` |  |
| `error` | `string` |  |

## state

### `flowgraph.state.accumulator`

**Accumulator** — push で input を List に蓄積、clear で空に戻す

| Input | Type | Default | Note |
|---|---|---|---|
| `push` | `exec` (in) | — |  |
| `clear` | `exec` (in) | — |  |
| `input` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `items` | `list<json>` |  |
| `count` | `int` |  |
| `updated` | `exec` (out) |  |
| `cleared` | `exec` (out) |  |

### `flowgraph.state.bool`

**Bool State** — set_true/set_false/toggle でブール値を切り替える

| Input | Type | Default | Note |
|---|---|---|---|
| `set_true` | `exec` (in) | — |  |
| `set_false` | `exec` (in) | — |  |
| `toggle` | `exec` (in) | — |  |

| Output | Type | Note |
|---|---|---|
| `value` | `bool` |  |
| `changed` | `exec` (out) |  |

### `flowgraph.state.int_counter`

**Int Counter** — step だけ増減/リセット可能な整数カウンタ

| Input | Type | Default | Note |
|---|---|---|---|
| `increment` | `exec` (in) | — |  |
| `decrement` | `exec` (in) | — |  |
| `reset` | `exec` (in) | — |  |
| `step` | `int` | `1` |  |
| `reset_value` | `int` | `0` |  |

| Output | Type | Note |
|---|---|---|
| `value` | `int` |  |
| `changed` | `exec` (out) |  |

### `flowgraph.state.latch`

**Latch** — exec_in で input を保持。以後 value 出力は保持値を返す

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `input` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `value` | `json` |  |
| `has_value` | `bool` |  |
| `updated` | `exec` (out) |  |

## string

### `flowgraph.string.concat`

**String Concat** — a と b を連結

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `string` | — |  |
| `b` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |

### `flowgraph.string.contains`

**String Contains**

| Input | Type | Default | Note |
|---|---|---|---|
| `haystack` | `string` | — |  |
| `needle` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.string.join`

**String Join** — List<String> を sep で連結

| Input | Type | Default | Note |
|---|---|---|---|
| `parts` | `list<string>` | — |  |
| `sep` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |

### `flowgraph.string.len`

**String Length** — UTF-8 文字数（chars().count()）

| Input | Type | Default | Note |
|---|---|---|---|
| `s` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `len` | `int` |  |

### `flowgraph.string.replace`

**String Replace** — pattern (リテラル) を replacement に全置換

| Input | Type | Default | Note |
|---|---|---|---|
| `s` | `string` | — |  |
| `pattern` | `string` | — |  |
| `replacement` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |

### `flowgraph.string.split`

**String Split** — sep で split して List<String> を返す

| Input | Type | Default | Note |
|---|---|---|---|
| `s` | `string` | — |  |
| `sep` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `parts` | `list<string>` |  |

## table

### `flowgraph.table.from_json`

**Table From JSON** — List<Json> (object の配列) を Table に変換。スキーマは先頭 object から推論

| Input | Type | Default | Note |
|---|---|---|---|
| `json` | `list<json>` | `[]` |  |

| Output | Type | Note |
|---|---|---|
| `table` | `table` |  |
| `row_count` | `int` |  |

### `flowgraph.table.load_tsv`

**Table Load TSV** — TSV ファイルを Table に読み込む（auto / headerful / legacy_loose）。Effectful

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `path` | `string` | — |  |
| `mode` | `string` | `"auto"` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `table` | `table` |  |
| `row_count` | `int` |  |
| `error` | `string` |  |

### `flowgraph.table.to_json`

**Table To JSON** — Table を List<Json> (object の配列) に変換

| Input | Type | Default | Note |
|---|---|---|---|
| `table` | `table` | `[]` |  |

| Output | Type | Note |
|---|---|---|
| `json` | `list<json>` |  |
| `row_count` | `int` |  |

### `flowgraph.table.write_tsv`

**Table Write TSV** — Table を TSV ファイルに書き出す（atomic rename）。Effectful

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `table` | `table` | — |  |
| `path` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_written` | `int` |  |
| `error` | `string` |  |

## translate

### `flowgraph.translate.gas`

**Translate (GAS)** — Google Apps Script 経由の翻訳 API 呼び出し

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `text` | `string` | — |  |
| `translate_to` | `string` | — |  |
| `translate_from` | `string` | `""` |  |
| `script_id` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `translated` | `string` |  |
| `detected_lang` | `string` |  |
| `error` | `string` |  |

### `flowgraph.translate.libre`

**Translate (LibreTranslate)** — LibreTranslate REST API (/translate) を叩く

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `text` | `string` | — |  |
| `translate_to` | `string` | — |  |
| `translate_from` | `string` | `""` |  |
| `base_url` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `translated` | `string` |  |
| `detected_lang` | `string` |  |
| `error` | `string` |  |

## tts

### `flowgraph.tts.speak`

**TTS: Speak** — engine 入力で選んだ TTS ドライバに text を合成させ、VAC 共有 audio_sink で再生する。save_path 指定時は WAV も保存。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `engine` | `string` | — |  |
| `text` | `string` | — |  |
| `voice` | `string` | `""` |  |
| `speed` | `float` | `1.0` |  |
| `pitch` | `float` | `0.0` |  |
| `volume` | `float` | `1.0` |  |
| `endpoint` | `string` | `""` |  |
| `extra` | `map<json>` | `{}` |  |
| `save_path` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `played` | `bool` |  |
| `audio_path` | `string` |  |
| `error` | `string` |  |

## twitch

### `flowgraph.twitch.ban`

**Twitch: Ban User** — Helix POST /moderation/bans（永久 ban）。`duration` なしで送る。timeout は `twitch.timeout` ノードを使うこと

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `user_id` | `string` | — |  |
| `broadcaster_id` | `string` | — |  |
| `moderator_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `reason` | `string` | `""` |  |
| `endpoint` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `end_time` | `string` |  |
| `error` | `string` |  |

### `flowgraph.twitch.chat_send`

**Twitch: Chat Send** — Helix POST /chat/messages でチャット送信。max_chars/strip_substrings は optional で V1 互換。レート制限は flowgraph.util.rate_limit を上流に挿入して実現する

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `text` | `string` | — |  |
| `broadcaster_id` | `string` | — |  |
| `sender_user_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `max_chars` | `int` | `500` |  |
| `strip_substrings` | `list<string>` | `[]` |  |
| `endpoint` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `on_skipped` | `exec` (out) |  |
| `sent_text` | `string` |  |
| `error` | `string` |  |

### `flowgraph.twitch.get_token`

**Twitch: Get Token** — conf.twitch で定義された token_key から保存済み OAuth トークンを取り出し、on_success / on_failure で分岐する

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `token_key` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_failure` | `exec` (out) |  |
| `access_token` | `string` |  |
| `client_id` | `string` |  |
| `error` | `string` |  |

### `flowgraph.twitch.timeout`

**Twitch: Timeout User** — Helix POST /moderation/bans（時間制限 ban）。`duration_secs` は 1..=1209600（最大 2 週間）。永久 ban は `twitch.ban`

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `user_id` | `string` | — |  |
| `broadcaster_id` | `string` | — |  |
| `moderator_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `reason` | `string` | `""` |  |
| `endpoint` | `string` | `""` |  |
| `duration_secs` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `end_time` | `string` |  |
| `error` | `string` |  |

### `flowgraph.twitch.user_id_by_login`

**Twitch: User ID by Login** — Helix GET /users?login=... で login から user_id を解決する

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `login` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `user_id` | `string` |  |
| `error` | `string` |  |

### `flowgraph.twitch.validate_token`

**Twitch: Validate Token** — GET https://id.twitch.tv/oauth2/validate で token 所有者の user_id / login / client_id を取得する

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `access_token` | `string` | — |  |
| `endpoint` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `user_id` | `string` |  |
| `login` | `string` |  |
| `client_id` | `string` |  |
| `error` | `string` |  |

## unit

### `flowgraph.unit.assign`

**Unit Assign** — Attach a unit to a dimensionless Float and produce a Quantity.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `unit` | `string` | `""` |  | SI-compatible unit string, e.g. "m/s^2", "Hz", "kg". Empty = dimensionless. |

### `flowgraph.unit.convert`

**Unit Convert** — Convert a Quantity to the target unit. Errors if dimensions differ or K/ΔK semantics mismatch.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `target_unit` | `string` | `""` | ✔ | Target unit string. Must match the input dimension. |

### `flowgraph.unit.get_dim_string`

**Dimension -> String** — Return the canonical dimension string (e.g. "L·T^-2").

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `dim` | `string` |  |

### `flowgraph.unit.get_unit_string`

**Unit -> String** — Return the canonical unit string (e.g. "m/s^2", "Hz").

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `name` | `string` |  |

### `flowgraph.unit.same_dimension`

**Same Dimension?** — True iff both inputs carry the same physical dimension.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `bool` |  |

### `flowgraph.unit.strip`

**Unit Strip** — Explicit escape hatch: discard the unit and emit the raw numeric value as Float.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.unit.to_json`

**Unit -> JSON** — Serialize Quantity to JSON with `value`, `unit`, `dimension` fields (internal form).

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `json` | `json` |  |

## util

### `flowgraph.util.delay`

**Delay** — exec_in 発火で value を保持し、delay_ms 後に exec_out を発火

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `value` | `json` | `null` |  |
| `delay_ms` | `int` | `1000` |  |
| `__resume__` | `exec` (in) | — |  |
| `__pending_id__` | `int` | `-1` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `value` | `json` |  |

### `flowgraph.util.format`

**Format Quantity** — Quantity → String with explicit include_unit / precision / unit_override control. Default output is "{value} {unit}" matching the engine-level Quantity → String coerce.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `include_unit` | `bool` | `true` |  | Append " {unit}" suffix when the value is non-dimensionless. |
| `precision` | `int` | `-1` |  | Decimal places for the numeric part. -1 means use the default Display formatter (no forced precision). |
| `unit_override` | `string` | `""` |  | If non-empty, convert the Quantity to this unit before formatting (same-dimension only, errors otherwise). Useful for rendering in a different unit than upstream. |

### `flowgraph.util.log`

**Log** — value 入力を trace に書き出し exec_out を発火。`Quantity` を流した場合は engine 側で `"{value} {unit}"` 形式に自動文字列化される（dimensionless は数値のみ）。精度や単位の ON/OFF を制御したい場合は `flowgraph.util.format` を挟む。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `value` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |

### `flowgraph.util.rate_limit`

**Rate Limit** — N 回 / X ms のトークンバケットで exec_in をゲートする（超過時は on_deny）

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `max_count` | `int` | — |  |
| `window_ms` | `int` | `30000` |  |

| Output | Type | Note |
|---|---|---|
| `on_allow` | `exec` (out) |  |
| `on_deny` | `exec` (out) |  |
| `remaining` | `int` |  |

