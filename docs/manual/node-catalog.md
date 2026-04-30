# Flowgraph Node Catalog

VAC v2 Flowgraph の組み込みノード一覧。**本ファイルは自動生成**されるので、手で編集せずノード定義側を更新してからテストで再生成してください。

再生成:

```powershell
$env:BLESS_NODE_CATALOG="1"; cargo test --lib node_catalog_md_up_to_date
```

> 型の表記: `bool` / `int` / `float` / `string` / `bytes` / `json` / `list<T>` / `map<T>` / `exec`

## Index

- **bytes**
  - [`flowgraph.bytes.from_base64`](#flowgraph-bytes-from-base64) — Bytes From Base64
  - [`flowgraph.bytes.len`](#flowgraph-bytes-len) — Bytes Length
  - [`flowgraph.bytes.to_base64`](#flowgraph-bytes-to-base64) — Bytes To Base64
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
  - [`flowgraph.convert.bool_to_string`](#flowgraph-convert-bool-to-string) — Bool → String
  - [`flowgraph.convert.float_to_int`](#flowgraph-convert-float-to-int) — Float → Int
  - [`flowgraph.convert.float_to_string`](#flowgraph-convert-float-to-string) — Float → String
  - [`flowgraph.convert.int_to_float`](#flowgraph-convert-int-to-float) — Int → Float
  - [`flowgraph.convert.int_to_string`](#flowgraph-convert-int-to-string) — Int → String
  - [`flowgraph.convert.string_to_float`](#flowgraph-convert-string-to-float) — String → Float
  - [`flowgraph.convert.string_to_int`](#flowgraph-convert-string-to-int) — String → Int
- **datetime**
  - [`flowgraph.datetime.add_duration`](#flowgraph-datetime-add-duration) — DateTime + Duration
  - [`flowgraph.datetime.diff`](#flowgraph-datetime-diff) — DateTime - DateTime
  - [`flowgraph.datetime.epoch_ms`](#flowgraph-datetime-epoch-ms) — DateTime -> Epoch ms
  - [`flowgraph.datetime.format`](#flowgraph-datetime-format) — DateTime Format
  - [`flowgraph.datetime.from_epoch_ms`](#flowgraph-datetime-from-epoch-ms) — Epoch ms -> DateTime
  - [`flowgraph.datetime.now`](#flowgraph-datetime-now) — DateTime Now
  - [`flowgraph.datetime.parse`](#flowgraph-datetime-parse) — DateTime Parse
  - [`flowgraph.datetime.sub_duration`](#flowgraph-datetime-sub-duration) — DateTime - Duration
  - [`flowgraph.datetime.try_parse`](#flowgraph-datetime-try-parse) — DateTime Try Parse
- **easing**
  - [`flowgraph.easing.apply`](#flowgraph-easing-apply) — Easing apply
- **flow**
  - [`flowgraph.flow.branch`](#flowgraph-flow-branch) — Branch
  - [`flowgraph.flow.gate`](#flowgraph-flow-gate) — Gate
- **glossary**
  - [`flowgraph.glossary.forget`](#flowgraph-glossary-forget) — Glossary Forget
  - [`flowgraph.glossary.learn`](#flowgraph-glossary-learn) — Glossary Learn
  - [`flowgraph.glossary.match`](#flowgraph-glossary-match) — Glossary Match
  - [`flowgraph.glossary.replace`](#flowgraph-glossary-replace) — Glossary Replace
- **http**
  - [`flowgraph.http.request`](#flowgraph-http-request) — HTTP Request
- **ingress**
  - [`flowgraph.ingress.channel_subscribe`](#flowgraph-ingress-channel-subscribe) — Channel Subscribe Ingress
  - [`flowgraph.ingress.osc_udp`](#flowgraph-ingress-osc-udp) — OSC UDP Ingress
  - [`flowgraph.ingress.twitch`](#flowgraph-ingress-twitch) — Twitch Ingress
  - [`flowgraph.ingress.twitch_eventsub`](#flowgraph-ingress-twitch-eventsub) — Twitch EventSub Ingress
  - [`flowgraph.ingress.vmc_udp`](#flowgraph-ingress-vmc-udp) — VMC UDP Ingress
  - [`flowgraph.ingress.voice`](#flowgraph-ingress-voice) — Voice Ingress
  - [`flowgraph.ingress.web_input`](#flowgraph-ingress-web-input) — Web Input Ingress
- **json**
  - [`flowgraph.json.get`](#flowgraph-json-get) — JSON Get
  - [`flowgraph.json.parse`](#flowgraph-json-parse) — JSON Parse
  - [`flowgraph.json.stringify`](#flowgraph-json-stringify) — JSON Stringify
  - [`flowgraph.json.try_parse`](#flowgraph-json-try-parse) — JSON Try Parse
- **library**
  - [`flowgraph.library.input`](#flowgraph-library-input) — Library Input
  - [`flowgraph.library.output`](#flowgraph-library-output) — Library Output
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
  - [`flowgraph.math.abs_float`](#flowgraph-math-abs-float) — Float abs
  - [`flowgraph.math.abs_int`](#flowgraph-math-abs-int) — Int abs
  - [`flowgraph.math.acos`](#flowgraph-math-acos) — Float acos
  - [`flowgraph.math.acosh`](#flowgraph-math-acosh) — Float acosh
  - [`flowgraph.math.asin`](#flowgraph-math-asin) — Float asin
  - [`flowgraph.math.asinh`](#flowgraph-math-asinh) — Float asinh
  - [`flowgraph.math.atan`](#flowgraph-math-atan) — Float atan
  - [`flowgraph.math.atan2`](#flowgraph-math-atan2) — Float atan2
  - [`flowgraph.math.atanh`](#flowgraph-math-atanh) — Float atanh
  - [`flowgraph.math.ceil`](#flowgraph-math-ceil) — Float ceil
  - [`flowgraph.math.clamp_float`](#flowgraph-math-clamp-float) — Float clamp
  - [`flowgraph.math.clamp_int`](#flowgraph-math-clamp-int) — Int clamp
  - [`flowgraph.math.cos`](#flowgraph-math-cos) — Float cos
  - [`flowgraph.math.cosh`](#flowgraph-math-cosh) — Float cosh
  - [`flowgraph.math.deg_to_rad`](#flowgraph-math-deg-to-rad) — Float deg → rad
  - [`flowgraph.math.exp`](#flowgraph-math-exp) — Float exp
  - [`flowgraph.math.float_add`](#flowgraph-math-float-add) — Float +
  - [`flowgraph.math.float_div`](#flowgraph-math-float-div) — Float /
  - [`flowgraph.math.float_mul`](#flowgraph-math-float-mul) — Float *
  - [`flowgraph.math.float_sub`](#flowgraph-math-float-sub) — Float -
  - [`flowgraph.math.floor`](#flowgraph-math-floor) — Float floor
  - [`flowgraph.math.int_add`](#flowgraph-math-int-add) — Int +
  - [`flowgraph.math.int_div`](#flowgraph-math-int-div) — Int /
  - [`flowgraph.math.int_max`](#flowgraph-math-int-max) — Int max
  - [`flowgraph.math.int_min`](#flowgraph-math-int-min) — Int min
  - [`flowgraph.math.int_mod`](#flowgraph-math-int-mod) — Int %
  - [`flowgraph.math.int_mul`](#flowgraph-math-int-mul) — Int *
  - [`flowgraph.math.int_sub`](#flowgraph-math-int-sub) — Int -
  - [`flowgraph.math.inverse_lerp`](#flowgraph-math-inverse-lerp) — Float inverse_lerp
  - [`flowgraph.math.lerp`](#flowgraph-math-lerp) — Float lerp
  - [`flowgraph.math.ln`](#flowgraph-math-ln) — Float ln
  - [`flowgraph.math.log10`](#flowgraph-math-log10) — Float log10
  - [`flowgraph.math.log2`](#flowgraph-math-log2) — Float log2
  - [`flowgraph.math.max_float`](#flowgraph-math-max-float) — Float max
  - [`flowgraph.math.min_float`](#flowgraph-math-min-float) — Float min
  - [`flowgraph.math.normalize_angle_deg_0_360`](#flowgraph-math-normalize-angle-deg-0-360) — Normalize angle [0, 360) deg
  - [`flowgraph.math.normalize_angle_deg_signed`](#flowgraph-math-normalize-angle-deg-signed) — Normalize angle [-180, 180) deg
  - [`flowgraph.math.normalize_angle_rad_0_2pi`](#flowgraph-math-normalize-angle-rad-0-2pi) — Normalize angle [0, 2π) rad
  - [`flowgraph.math.normalize_angle_rad_signed`](#flowgraph-math-normalize-angle-rad-signed) — Normalize angle [-π, π) rad
  - [`flowgraph.math.pow`](#flowgraph-math-pow) — Float pow
  - [`flowgraph.math.rad_to_deg`](#flowgraph-math-rad-to-deg) — Float rad → deg
  - [`flowgraph.math.remap`](#flowgraph-math-remap) — Float remap
  - [`flowgraph.math.round`](#flowgraph-math-round) — Float round
  - [`flowgraph.math.sign_float`](#flowgraph-math-sign-float) — Float sign
  - [`flowgraph.math.sign_int`](#flowgraph-math-sign-int) — Int sign
  - [`flowgraph.math.sin`](#flowgraph-math-sin) — Float sin
  - [`flowgraph.math.sinh`](#flowgraph-math-sinh) — Float sinh
  - [`flowgraph.math.smoothstep`](#flowgraph-math-smoothstep) — Float smoothstep
  - [`flowgraph.math.sqrt`](#flowgraph-math-sqrt) — Float sqrt
  - [`flowgraph.math.tan`](#flowgraph-math-tan) — Float tan
  - [`flowgraph.math.tanh`](#flowgraph-math-tanh) — Float tanh
- **mode**
  - [`flowgraph.mode.equals`](#flowgraph-mode-equals) — Mode Equals
  - [`flowgraph.mode.get`](#flowgraph-mode-get) — Mode Get
  - [`flowgraph.mode.transit`](#flowgraph-mode-transit) — Mode Transit
- **motion**
  - [`flowgraph.motion.filter`](#flowgraph-motion-filter) — Motion: Filter OSC Messages
  - [`flowgraph.motion.map`](#flowgraph-motion-map) — Motion: Map Numeric Args
  - [`flowgraph.motion.vmc_parse`](#flowgraph-motion-vmc-parse) — Motion: VMC OSC Parse
- **noise**
  - [`flowgraph.noise.perlin_1d`](#flowgraph-noise-perlin-1d) — Perlin 1D
  - [`flowgraph.noise.perlin_2d`](#flowgraph-noise-perlin-2d) — Perlin 2D
- **obs**
  - [`flowgraph.obs.get_current_program_scene`](#flowgraph-obs-get-current-program-scene) — OBS: Get Current Program Scene
  - [`flowgraph.obs.request`](#flowgraph-obs-request) — OBS: Request
  - [`flowgraph.obs.set_current_program_scene`](#flowgraph-obs-set-current-program-scene) — OBS: Set Current Program Scene
  - [`flowgraph.obs.set_scene_item_enabled`](#flowgraph-obs-set-scene-item-enabled) — OBS: Set Scene Item Enabled
  - [`flowgraph.obs.start_record`](#flowgraph-obs-start-record) — OBS: Start Record
  - [`flowgraph.obs.start_stream`](#flowgraph-obs-start-stream) — OBS: Start Stream
  - [`flowgraph.obs.stop_record`](#flowgraph-obs-stop-record) — OBS: Stop Record
  - [`flowgraph.obs.stop_stream`](#flowgraph-obs-stop-stream) — OBS: Stop Stream
  - [`flowgraph.obs.trigger_studio_mode_transition`](#flowgraph-obs-trigger-studio-mode-transition) — OBS: Trigger Studio Mode Transition
- **ocr**
  - [`flowgraph.ocr.recognize`](#flowgraph-ocr-recognize) — OCR Recognize
- **osc**
  - [`flowgraph.osc.send`](#flowgraph-osc-send) — OSC: UDP Send
- **process**
  - [`flowgraph.process.kill`](#flowgraph-process-kill) — Process: Kill
  - [`flowgraph.process.running`](#flowgraph-process-running) — Process: Running
  - [`flowgraph.process.spawn`](#flowgraph-process-spawn) — Process: Spawn
  - [`flowgraph.process.wait`](#flowgraph-process-wait) — Process: Wait
- **random**
  - [`flowgraph.random.normal`](#flowgraph-random-normal) — Random normal
  - [`flowgraph.random.uniform_float`](#flowgraph-random-uniform-float) — Random uniform (float)
  - [`flowgraph.random.uniform_int`](#flowgraph-random-uniform-int) — Random uniform (int)
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
- **system**
  - [`flowgraph.system.cpu_usage`](#flowgraph-system-cpu-usage) — System: CPU Usage
  - [`flowgraph.system.load_avg`](#flowgraph-system-load-avg) — System: Load Average
  - [`flowgraph.system.memory`](#flowgraph-system-memory) — System: Memory
  - [`flowgraph.system.process_list`](#flowgraph-system-process-list) — System: Process List
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
  - [`flowgraph.twitch.ad_run`](#flowgraph-twitch-ad-run) — Twitch: Run Ad
  - [`flowgraph.twitch.ban`](#flowgraph-twitch-ban) — Twitch: Ban User
  - [`flowgraph.twitch.channel_info_update`](#flowgraph-twitch-channel-info-update) — Twitch: Update Channel Info
  - [`flowgraph.twitch.chat_clear`](#flowgraph-twitch-chat-clear) — Twitch: Clear Chat
  - [`flowgraph.twitch.chat_send`](#flowgraph-twitch-chat-send) — Twitch: Chat Send
  - [`flowgraph.twitch.chat_settings_update`](#flowgraph-twitch-chat-settings-update) — Twitch: Update Chat Settings
  - [`flowgraph.twitch.clip_create`](#flowgraph-twitch-clip-create) — Twitch: Create Clip
  - [`flowgraph.twitch.get_token`](#flowgraph-twitch-get-token) — Twitch: Get Token
  - [`flowgraph.twitch.goals_get`](#flowgraph-twitch-goals-get) — Twitch: Get Goals
  - [`flowgraph.twitch.poll_create`](#flowgraph-twitch-poll-create) — Twitch: Create Poll
  - [`flowgraph.twitch.poll_end`](#flowgraph-twitch-poll-end) — Twitch: End Poll
  - [`flowgraph.twitch.prediction_create`](#flowgraph-twitch-prediction-create) — Twitch: Create Prediction
  - [`flowgraph.twitch.prediction_end`](#flowgraph-twitch-prediction-end) — Twitch: End Prediction
  - [`flowgraph.twitch.raid_cancel`](#flowgraph-twitch-raid-cancel) — Twitch: Cancel Raid
  - [`flowgraph.twitch.raid_start`](#flowgraph-twitch-raid-start) — Twitch: Start Raid
  - [`flowgraph.twitch.shield_mode_update`](#flowgraph-twitch-shield-mode-update) — Twitch: Update Shield Mode
  - [`flowgraph.twitch.stream_marker_create`](#flowgraph-twitch-stream-marker-create) — Twitch: Create Stream Marker
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
  - [`flowgraph.unit.try_parse`](#flowgraph-unit-try-parse) — Unit Try Parse
- **util**
  - [`flowgraph.util.debounce`](#flowgraph-util-debounce) — Debounce
  - [`flowgraph.util.delay`](#flowgraph-util-delay) — Delay
  - [`flowgraph.util.edge_detect`](#flowgraph-util-edge-detect) — Edge detect
  - [`flowgraph.util.format`](#flowgraph-util-format) — Format Quantity
  - [`flowgraph.util.log`](#flowgraph-util-log) — Log
  - [`flowgraph.util.prev_value`](#flowgraph-util-prev-value) — Previous value
  - [`flowgraph.util.rate_limit`](#flowgraph-util-rate-limit) — Rate Limit
  - [`flowgraph.util.sample_hold`](#flowgraph-util-sample-hold) — Sample & hold
  - [`flowgraph.util.throttle`](#flowgraph-util-throttle) — Throttle
  - [`flowgraph.util.timer_interval`](#flowgraph-util-timer-interval) — Timer Interval
- **vec**
  - [`flowgraph.vec2.add`](#flowgraph-vec2-add) — Vec2 add
  - [`flowgraph.vec2.distance`](#flowgraph-vec2-distance) — Vec2 distance
  - [`flowgraph.vec2.dot`](#flowgraph-vec2-dot) — Vec2 dot
  - [`flowgraph.vec2.length`](#flowgraph-vec2-length) — Vec2 length
  - [`flowgraph.vec2.lerp`](#flowgraph-vec2-lerp) — Vec2 lerp
  - [`flowgraph.vec2.make`](#flowgraph-vec2-make) — Vec2 make
  - [`flowgraph.vec2.normalize`](#flowgraph-vec2-normalize) — Vec2 normalize
  - [`flowgraph.vec2.scale`](#flowgraph-vec2-scale) — Vec2 scale
  - [`flowgraph.vec2.sub`](#flowgraph-vec2-sub) — Vec2 sub
  - [`flowgraph.vec2.unpack`](#flowgraph-vec2-unpack) — Vec2 unpack
  - [`flowgraph.vec3.add`](#flowgraph-vec3-add) — Vec3 add
  - [`flowgraph.vec3.distance`](#flowgraph-vec3-distance) — Vec3 distance
  - [`flowgraph.vec3.dot`](#flowgraph-vec3-dot) — Vec3 dot
  - [`flowgraph.vec3.length`](#flowgraph-vec3-length) — Vec3 length
  - [`flowgraph.vec3.lerp`](#flowgraph-vec3-lerp) — Vec3 lerp
  - [`flowgraph.vec3.make`](#flowgraph-vec3-make) — Vec3 make
  - [`flowgraph.vec3.normalize`](#flowgraph-vec3-normalize) — Vec3 normalize
  - [`flowgraph.vec3.scale`](#flowgraph-vec3-scale) — Vec3 scale
  - [`flowgraph.vec3.sub`](#flowgraph-vec3-sub) — Vec3 sub
  - [`flowgraph.vec3.unpack`](#flowgraph-vec3-unpack) — Vec3 unpack
- **vmc**
  - [`flowgraph.vmc.extract_blendshape`](#flowgraph-vmc-extract-blendshape) — VMC: Extract BlendShape
  - [`flowgraph.vmc.extract_bone_pos`](#flowgraph-vmc-extract-bone-pos) — VMC: Extract Bone Pos
  - [`flowgraph.vmc.extract_root_pos`](#flowgraph-vmc-extract-root-pos) — VMC: Extract Root Pos
  - [`flowgraph.vmc.send_bone_pos`](#flowgraph-vmc-send-bone-pos) — VMC: Send Bone Pos
  - [`flowgraph.vmc.send_root_pos`](#flowgraph-vmc-send-root-pos) — VMC: Send Root Pos
- **vrchat**
  - [`flowgraph.vrchat.avatar_parameter_bool`](#flowgraph-vrchat-avatar-parameter-bool) — VRChat: Avatar Parameter (Bool)
  - [`flowgraph.vrchat.avatar_parameter_float`](#flowgraph-vrchat-avatar-parameter-float) — VRChat: Avatar Parameter (Float)
  - [`flowgraph.vrchat.avatar_parameter_int`](#flowgraph-vrchat-avatar-parameter-int) — VRChat: Avatar Parameter (Int)
  - [`flowgraph.vrchat.chatbox_input`](#flowgraph-vrchat-chatbox-input) — VRChat: Chatbox Input
  - [`flowgraph.vrchat.chatbox_typing`](#flowgraph-vrchat-chatbox-typing) — VRChat: Chatbox Typing
- **window**
  - [`flowgraph.window.close`](#flowgraph-window-close) — Window: Close
  - [`flowgraph.window.enum`](#flowgraph-window-enum) — Window: Enum
  - [`flowgraph.window.foreground`](#flowgraph-window-foreground) — Window: Foreground
  - [`flowgraph.window.maximize`](#flowgraph-window-maximize) — Window: Maximize
  - [`flowgraph.window.minimize`](#flowgraph-window-minimize) — Window: Minimize
  - [`flowgraph.window.move`](#flowgraph-window-move) — Window: Move
  - [`flowgraph.window.pseudo_fullscreen`](#flowgraph-window-pseudo-fullscreen) — Window: Pseudo Fullscreen
  - [`flowgraph.window.pseudo_fullscreen_exit`](#flowgraph-window-pseudo-fullscreen-exit) — Window: Pseudo Fullscreen Exit
  - [`flowgraph.window.resize`](#flowgraph-window-resize) — Window: Resize
  - [`flowgraph.window.restore`](#flowgraph-window-restore) — Window: Restore

## bytes

### `flowgraph.bytes.from_base64`

**Bytes From Base64** — Base64 string を bytes にデコードする。失敗時はエラー halt。

| Input | Type | Default | Note |
|---|---|---|---|
| `text` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `bytes` | `bytes` |  |

### `flowgraph.bytes.len`

**Bytes Length** — bytes の byte length を返す。

| Input | Type | Default | Note |
|---|---|---|---|
| `bytes` | `bytes` | — |  |

| Output | Type | Note |
|---|---|---|
| `len` | `int` |  |

### `flowgraph.bytes.to_base64`

**Bytes To Base64** — bytes を Base64 string にエンコードする。

| Input | Type | Default | Note |
|---|---|---|---|
| `bytes` | `bytes` | — |  |

| Output | Type | Note |
|---|---|---|
| `text` | `string` |  |

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
| `result` | `result<bool>` |  |

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

### `flowgraph.convert.bool_to_string`

**Bool → String**

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |

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

## datetime

### `flowgraph.datetime.add_duration`

**DateTime + Duration** — Add a duration (Quantity<time>) to a DateTime. Dimensionless Quantity (Float の ξ-3 coerce 経由) は「秒」と解釈される。次元不一致 (例: length) は error。

| Input | Type | Default | Note |
|---|---|---|---|
| `datetime` | `datetime` | — |  |
| `duration` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `datetime` |  |

### `flowgraph.datetime.diff`

**DateTime - DateTime** — Compute `lhs - rhs` as a Quantity<time> (unit: seconds, nanosecond precision). 結果は正負 OK。`flowgraph.unit.convert` で ms / us / ns に変換可能。

| Input | Type | Default | Note |
|---|---|---|---|
| `lhs` | `datetime` | — |  |
| `rhs` | `datetime` | — |  |

| Output | Type | Note |
|---|---|---|
| `duration` | `quantity` |  |

### `flowgraph.datetime.epoch_ms`

**DateTime -> Epoch ms** — Return Unix epoch milliseconds as a Quantity (unit: ms, dim: time). Negative for pre-1970 timestamps. 他の時間単位へは `flowgraph.unit.convert` で変換。

| Input | Type | Default | Note |
|---|---|---|---|
| `datetime` | `datetime` | — |  |

| Output | Type | Note |
|---|---|---|
| `millis` | `quantity` |  |

### `flowgraph.datetime.format`

**DateTime Format** — Format a DateTime as a string. `rfc3339`: `"2026-04-24T12:34:56.123Z"` 形式 (UTC または `timezone` 指定時は offset 表示)。 `iso8601_compact`: `"20260424T123456Z"` 形式 (ファイル名向け)。 `unix_seconds` / `unix_millis`: 整数文字列。 `custom`: `custom_format` プロパティの strftime パターンを適用 (jiff::Zoned::strftime)。

| Input | Type | Default | Note |
|---|---|---|---|
| `datetime` | `datetime` | — |  |

| Output | Type | Note |
|---|---|---|
| `text` | `string` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `format` | `string` | `"rfc3339"` |  | Output shape. `rfc3339` / `iso8601_compact` / `unix_seconds` / `unix_millis` / `custom`. |
| `custom_format` | `string` | `""` |  | strftime pattern used when `format = "custom"`. See jiff::fmt::strtime. Example: `"%Y-%m-%d %H:%M:%S"`. |
| `timezone` | `string` | `""` |  | Fixed offset for display (`""` / `"Z"` / `"UTC"` = UTC, `"+09:00"` etc.). Applies to rfc3339 / iso8601_compact / custom. unix_* are always UTC-absolute and ignore this. |

### `flowgraph.datetime.from_epoch_ms`

**Epoch ms -> DateTime** — Construct a DateTime from Unix epoch milliseconds. Quantity<time> (任意の時間単位) は SI 秒 → ms に正規化されて受理される。 Dimensionless Quantity (Float の ξ-3 coerce 経由) は「ms の数値」として解釈される。

| Input | Type | Default | Note |
|---|---|---|---|
| `millis` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `datetime` | `datetime` |  |

### `flowgraph.datetime.now`

**DateTime Now** — Emit the current wall-clock time as a DateTime (UTC absolute, nanosecond precision). 非決定性 (呼び出しごとに異なる結果) なので、すなゆく sample する用途では状態化ノード (prev_value 等) と組み合わせること。

| Output | Type | Note |
|---|---|---|
| `datetime` | `datetime` |  |

### `flowgraph.datetime.parse`

**DateTime Parse** — Parse an RFC3339 / ISO 8601 string into a DateTime. Accepts both aware ("...Z" / "...+09:00") and naive ("2026-04-24T12:34:56") inputs. Naive 入力は `default_timezone` プロパティ (空なら UTC) で解釈される。`require_timezone = true` のときは naive を拒否する strict モード。

| Input | Type | Default | Note |
|---|---|---|---|
| `s` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `datetime` | `datetime` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `require_timezone` | `bool` | `false` |  | When true, naive (no-timezone) inputs are rejected. Default false: naive inputs are interpreted with `default_timezone` (or UTC). |
| `default_timezone` | `string` | `""` |  | Fixed offset to apply when the input has no timezone info. Accepts `""` / `"Z"` / `"UTC"` (= UTC), `"+09:00"`, `"-05:30"`. IANA zones (`"Asia/Tokyo"`) are rejected (v0 is fixed-offset only). |

### `flowgraph.datetime.sub_duration`

**DateTime - Duration** — Subtract a duration (Quantity<time>) from a DateTime. Dimensionless Quantity は「秒」と解釈される (ξ-3 coerce)。

| Input | Type | Default | Note |
|---|---|---|---|
| `datetime` | `datetime` | — |  |
| `duration` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `datetime` |  |

### `flowgraph.datetime.try_parse`

**DateTime Try Parse** — Parse a datetime string and return failure as result<datetime> instead of halting.

| Input | Type | Default | Note |
|---|---|---|---|
| `s` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `ok` | `bool` |  |
| `datetime` | `datetime` |  |
| `error` | `string` |  |
| `result` | `result<datetime>` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `require_timezone` | `bool` | `false` |  | When true, naive (no-timezone) inputs are rejected. Default false: naive inputs are interpreted with `default_timezone` (or UTC). |
| `default_timezone` | `string` | `""` |  | Fixed offset to apply when the input has no timezone info. Accepts `""` / `"Z"` / `"UTC"` (= UTC), `"+09:00"`, `"-05:30"`. IANA zones (`"Asia/Tokyo"`) are rejected (v0 is fixed-offset only). |

## easing

### `flowgraph.easing.apply`

**Easing apply** — Map a scalar t (conventionally in [0, 1]) through an easing curve. Single node with a `curve` enum property (19 variants: linear, quad/cubic/sine/expo/elastic/bounce × in/out/inout). `clamp_t = true` (default) clamps input to [0, 1] before evaluation — set false to let elastic/bounce overshoot naturally.

| Input | Type | Default | Note |
|---|---|---|---|
| `t` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `value` | `float` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `curve` | `string` | `"linear"` |  | Easing curve name. One of: linear, quad_in/out/inout, cubic_in/out/inout, sine_in/out/inout, expo_in/out/inout, elastic_in/out/inout, bounce_in/out/inout. |
| `clamp_t` | `bool` | `true` |  | When true, t is clamped to [0, 1] before the curve is applied. Default true. |

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

## glossary

### `flowgraph.glossary.forget`

**Glossary Forget** — Glossary Table から source (+ replacement) 一致行を削除。mode=latest/all/exact、is_locked 保護

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

### `flowgraph.glossary.learn`

**Glossary Learn** — Glossary Table に 11 カラムエントリを append。同値エントリは duplicate 検出して no-op

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

### `flowgraph.glossary.match`

**Glossary Match** — Glossary Table で text を照合し、一致エントリと captures を取り出す。exec 分岐可能。Stateful

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

### `flowgraph.glossary.replace`

**Glossary Replace** — Glossary Table（11 カラム）で content を literal(AC) + regex 統合で逐次置換。Stateful（AC/Regex キャッシュ）

| Input | Type | Default | Note |
|---|---|---|---|
| `content` | `string` | — |  |
| `dictionary` | `table` | `[]` |  |

| Output | Type | Note |
|---|---|---|
| `result` | `string` |  |
| `applied_count` | `int` |  |

## http

### `flowgraph.http.request`

**HTTP Request** — 汎用 HTTP request。method / headers / JSON body / timeout / retry_count を指定できる。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `method` | `string` | `"GET"` |  |
| `url` | `string` | — |  |
| `headers` | `json` | `{}` |  |
| `body` | `json` | `null` |  |
| `timeout_ms` | `int` | `5000` |  |
| `retry_count` | `int` | `0` |  |
| `retry_delay_ms` | `int` | `250` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status` | `int` |  |
| `body` | `string` |  |
| `json` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

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

### `flowgraph.ingress.osc_udp`

**OSC UDP Ingress** — 汎用 OSC（UDP データグラム）を受信し、ingress echo で下流へ流す。`content` は Base64、`content_bytes` は bytes。`__meta__.profile` は `osc_udp`。

| Input | Type | Default | Note |
|---|---|---|---|
| `__trigger__` | `exec` (in) | — |  |
| `__content__` | `string` | `""` |  |
| `__source_actor__` | `string` | `""` |  |
| `__source_kind__` | `string` | `""` |  |
| `__meta__` | `json` | `null` |  |
| `__content_bytes__` | `bytes` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `content` | `string` |  |
| `source_actor` | `string` |  |
| `source_kind` | `string` |  |
| `meta` | `json` |  |
| `content_bytes` | `bytes` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `bind` | `string` | `""` |  | 受信 UDP の "host:port"。空のときブリッジは起動しない。 |
| `fixed_channel` | `string` | `""` |  | 空なら `source_kind` は `osc_udp`。任意のラベルに上書き可能。 |

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

### `flowgraph.ingress.vmc_udp`

**VMC UDP Ingress** — VMC 互換の生 UDP を受信し、各データグラムを ingress echo で下流へ流す。`content` は Base64 文字列、`content_bytes` は bytes。

| Input | Type | Default | Note |
|---|---|---|---|
| `__trigger__` | `exec` (in) | — |  |
| `__content__` | `string` | `""` |  |
| `__source_actor__` | `string` | `""` |  |
| `__source_kind__` | `string` | `""` |  |
| `__meta__` | `json` | `null` |  |
| `__content_bytes__` | `bytes` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `content` | `string` |  |
| `source_actor` | `string` |  |
| `source_kind` | `string` |  |
| `meta` | `json` |  |
| `content_bytes` | `bytes` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `bind` | `string` | `""` |  | 受信 UDP の "host:port"（例 "0.0.0.0:39539"）。空のときブリッジは起動しない。 |
| `fixed_channel` | `string` | `""` |  | 空なら `source_kind` は `vmc_udp`。任意のラベルに上書き可能。 |

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

### `flowgraph.json.try_parse`

**JSON Try Parse** — String を JSON にパースし、失敗を result<json> として返す。

| Input | Type | Default | Note |
|---|---|---|---|
| `text` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `ok` | `bool` |  |
| `value` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

## library

### `flowgraph.library.input`

**Library Input** — Phase λ v0: 単一 string 境界（プロパティ value）。将来は接続駆動の動的ポートを予定。

| Output | Type | Note |
|---|---|---|
| `value` | `string` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `value` | `string` | `""` |  |  |

### `flowgraph.library.output`

**Library Output** — Phase λ v0: 単一 string 境界（入力 value を受ける）。将来は外向き動的ポートを予定。

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `string` | — |  |

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

### `flowgraph.math.abs_float`

**Float abs** — Absolute value. Unit is preserved (abs(-5 m) = 5 m). NaN input yields NaN output.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.abs_int`

**Int abs** — Integer absolute value. i64::MIN overflow is handled via wrapping_abs (returns i64::MIN).

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.acos`

**Float acos** — Inverse cosine. Input dimensionless. Output Angle (rad). |x| > 1 yields NaN.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.acosh`

**Float acosh** — Inverse hyperbolic cosine. Input must be dimensionless. x < 1 yields NaN (std::f64).

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.asin`

**Float asin** — Inverse sine. Input dimensionless. Output Angle (rad). |x| > 1 yields NaN (std::f64).

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.asinh`

**Float asinh** — Inverse hyperbolic sine. Input must be dimensionless. Defined for all real x.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.atan`

**Float atan** — Inverse tangent. Input dimensionless. Output Angle (rad) in (-π/2, π/2).

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.atan2`

**Float atan2** — Two-argument arctangent. y and x must share a dimension (so their ratio is dimensionless). Output is Angle (rad) in (-π, π].

| Input | Type | Default | Note |
|---|---|---|---|
| `y` | `quantity` | — |  |
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.atanh`

**Float atanh** — Inverse hyperbolic tangent. Input must be dimensionless. |x| ≥ 1 yields ±inf/NaN (std::f64).

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.ceil`

**Float ceil** — Smallest integer ≥ x (f64::ceil). Unit is preserved.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.clamp_float`

**Float clamp** — Clamp value to [lo, hi] on Quantity. All three inputs must share a dimension. If lo > hi after unit-normalizing into value's unit, they are swapped. Result unit follows the value input.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |
| `lo` | `quantity` | — |  |
| `hi` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.clamp_int`

**Int clamp** — Clamp value to [lo, hi]. If lo > hi, they are swapped before clamping.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `int` | — |  |
| `lo` | `int` | — |  |
| `hi` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.cos`

**Float cos** — Cosine. Input is Angle (rad/deg) or dimensionless (treated as radians). Output is dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.cosh`

**Float cosh** — Hyperbolic cosine. Input must be dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.deg_to_rad`

**Float deg → rad** — Convert degrees to radians. Angle-dimensioned input is converted via flowgraph.unit.convert semantics. Dimensionless input is scaled by π/180 and tagged with rad unit. Other dimensions are rejected.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.exp`

**Float exp** — e^x. Input must be dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.float_add`

**Float +** — Quantity addition. Dimension mismatch is an error. ΔK + K(abs) is allowed; K + K is rejected.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.float_div`

**Float /** — Quantity division. Dimensions are composed (m / s = m·s⁻¹). Absolute-temperature division and division by zero are rejected.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.float_mul`

**Float *** — Quantity multiplication. Dimensions are composed (m * s = m·s). Absolute-temperature multiplication is rejected.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.float_sub`

**Float -** — Quantity subtraction. Dimension mismatch is an error. K - K yields ΔK.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.floor`

**Float floor** — Largest integer ≤ x (f64::floor). Unit is preserved.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

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

### `flowgraph.math.int_max`

**Int max**

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `int` | — |  |
| `b` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.int_min`

**Int min**

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

### `flowgraph.math.inverse_lerp`

**Float inverse_lerp** — Inverse of lerp: (v - a) / (b - a). All three inputs share a dimension. If a == b (after unit normalization), returns 0.0. Result is dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |
| `v` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.lerp`

**Float lerp** — Linear interpolation: a + (b - a) * t. a and b share a dimension; t is dimensionless. t is not clamped (extrapolation allowed). Result unit follows a.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |
| `t` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.ln`

**Float ln** — Natural logarithm. Input must be dimensionless. x ≤ 0 yields -inf/NaN per std::f64.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.log10`

**Float log10** — Base-10 logarithm. Input must be dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.log2`

**Float log2** — Base-2 logarithm. Input must be dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.max_float`

**Float max** — Maximum of two same-dimension Quantity values. Result keeps A's unit. NaN follows f64::max semantics.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.min_float`

**Float min** — Minimum of two same-dimension Quantity values. Result keeps A's unit. NaN follows f64::min semantics (NaN propagates to other operand).

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `quantity` | — |  |
| `b` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.normalize_angle_deg_0_360`

**Normalize angle [0, 360) deg** — Normalize Angle into [0, 360) degrees. 1357.33 → 277.33. Angle-dim input is converted to deg first; dimensionless is treated as deg.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.normalize_angle_deg_signed`

**Normalize angle [-180, 180) deg** — Normalize Angle into [-180, +180) degrees. 277.33 → -82.67. Angle-dim input is converted to deg first; dimensionless is treated as deg.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.normalize_angle_rad_0_2pi`

**Normalize angle [0, 2π) rad** — Normalize Angle into [0, 2π) radians. Angle-dim input is converted to rad first; dimensionless is treated as rad.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.normalize_angle_rad_signed`

**Normalize angle [-π, π) rad** — Normalize Angle into [-π, +π) radians. Angle-dim input is converted to rad first; dimensionless is treated as rad.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.pow`

**Float pow** — base^exp. Both base and exp must be dimensionless (general Quantity pow requires an integer exponent for dimension algebra; for that use flowgraph.unit.* + custom). NaN and ±inf follow f64::powf semantics.

| Input | Type | Default | Note |
|---|---|---|---|
| `base` | `quantity` | — |  |
| `exp` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.rad_to_deg`

**Float rad → deg** — Convert radians to degrees. Angle-dimensioned input is converted via flowgraph.unit.convert semantics. Dimensionless input is scaled by 180/π and tagged with deg unit. Other dimensions are rejected.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.remap`

**Float remap** — Remap value from [in_lo, in_hi] to [out_lo, out_hi]. value/in_lo/in_hi share a dimension; out_lo/out_hi share another dimension. If in_lo == in_hi, out_lo is returned. Result unit follows out_lo.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `quantity` | — |  |
| `in_lo` | `quantity` | — |  |
| `in_hi` | `quantity` | — |  |
| `out_lo` | `quantity` | — |  |
| `out_hi` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.round`

**Float round** — Round half away from zero (f64::round std default). Unit is preserved.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.sign_float`

**Float sign** — Sign classifier: -1 / 0 / +1 (value only, unit preserved). NaN yields 0. Note: the SI meaning of a unit-bearing sign is unusual but kept for pass-through consistency.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.sign_int`

**Int sign** — Integer sign: -1 for x<0, 0 for x==0, +1 for x>0.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `int` |  |

### `flowgraph.math.sin`

**Float sin** — Sine. Input is Angle (rad/deg) or dimensionless (treated as radians for pre-ξ compatibility). Output is dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.sinh`

**Float sinh** — Hyperbolic sine. Input must be dimensionless (hyperbolic functions take unitless arguments in their SI-compatible form).

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.smoothstep`

**Float smoothstep** — GLSL smoothstep: t = clamp((x - edge0) / (edge1 - edge0), 0, 1); returns t*t*(3 - 2*t). All three inputs share a dimension. If edge0 == edge1, returns 0.0. Result is dimensionless in [0, 1].

| Input | Type | Default | Note |
|---|---|---|---|
| `edge0` | `quantity` | — |  |
| `edge1` | `quantity` | — |  |
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.sqrt`

**Float sqrt** — Square root. Dimension-aware: sqrt(m²) = m, sqrt(m²/s²) = m/s. All atom exponents must be even (the current type system only represents integer dimensions), so sqrt(m) is rejected — use flowgraph.unit.strip first if that was intentional. Absolute temperature (K) is rejected. Negative value yields NaN.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.tan`

**Float tan** — Tangent. Input is Angle (rad/deg) or dimensionless (treated as radians). Output is dimensionless. ±(π/2) yields large finite values per std::f64.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

### `flowgraph.math.tanh`

**Float tanh** — Hyperbolic tangent. Input must be dimensionless.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `quantity` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `quantity` |  |

## mode

### `flowgraph.mode.equals`

**Mode Equals** — 実効 Runtime Mode が expected と一致するか（前後空白は無視）。未上書き時は default_runtime_mode 相当と比較。

| Input | Type | Default | Note |
|---|---|---|---|
| `expected` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `equals` | `bool` |  |

### `flowgraph.mode.get`

**Mode Get** — 現在の実効 Runtime Mode ID。Control API で上書きした値。未上書き時は conf.default_runtime_mode に従う（空のことあり）。

| Output | Type | Note |
|---|---|---|
| `mode` | `string` |  |

### `flowgraph.mode.transit`

**Mode Transit** — 指定 mode へ Runtime Mode を切り替える（`State.runtime_mode_id` + TriggerGate 再計算）。`mode` が空なら default_runtime_mode 相当。`State` 未接続・conf 再読込失敗・未知 mode では on_reject。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `mode` | `string` | `""` |  |
| `reason` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_reject` | `exec` (out) |  |
| `accepted` | `bool` |  |
| `message` | `string` |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `noop_message` | `string` | `"noop"` |  | 実効 mode が変わらなかったときの `message` 文字列。 |

## motion

### `flowgraph.motion.filter`

**Motion: Filter OSC Messages** — `osc_messages` を `address_prefix` と `address_substring`（両方省略可）でフィルタする。入出力は `motion_frame`（`json` へ coerce 可）

| Input | Type | Default | Note |
|---|---|---|---|
| `frame` | `motion_frame` | — |  |
| `address_prefix` | `string` | `""` |  |
| `address_substring` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `frame_out` | `motion_frame` |  |

### `flowgraph.motion.map`

**Motion: Map Numeric Args** — 各 `osc_messages[].args` の JSON 数値を再帰的に `float_scale` 倍する。入出力は `motion_frame`（`json` へ coerce 可）

| Input | Type | Default | Note |
|---|---|---|---|
| `frame` | `motion_frame` | — |  |
| `float_scale` | `float` | `1.0` |  |

| Output | Type | Note |
|---|---|---|
| `frame_out` | `motion_frame` |  |

### `flowgraph.motion.vmc_parse`

**Motion: VMC OSC Parse** — UDP ペイロードを OSC として解釈し、[`MotionFrame`]（`motion_frame`）を出力。`payload` bytes が非空なら優先し、空なら互換用 `payload_b64` を読む。`json` へ接続時は自動変換

| Input | Type | Default | Note |
|---|---|---|---|
| `payload` | `bytes` | `""` |  |
| `payload_b64` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `frame` | `motion_frame` |  |

## noise

### `flowgraph.noise.perlin_1d`

**Perlin 1D** — 1D Perlin noise at coordinate `t` with integer `seed` (cached Perlin per seed).

| Input | Type | Default | Note |
|---|---|---|---|
| `t` | `float` | — |  |
| `seed` | `int` | `0` |  |

| Output | Type | Note |
|---|---|---|
| `value` | `float` |  |

### `flowgraph.noise.perlin_2d`

**Perlin 2D** — 2D Perlin noise at `(x, y)` with integer `seed`.

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `float` | — |  |
| `y` | `float` | — |  |
| `seed` | `int` | `0` |  |

| Output | Type | Note |
|---|---|---|
| `value` | `float` |  |

## obs

### `flowgraph.obs.get_current_program_scene`

**OBS: Get Current Program Scene** — OBS WebSocket v5 の `GetCurrentProgramScene` を呼び、現在の番組シーン名を返す。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |
| `scene_name` | `string` |  |

### `flowgraph.obs.request`

**OBS: Request** — OBS WebSocket v5 に 1 request を送る汎用ノード。OBS 側で WebSocket Server を有効化しておく。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `request_type` | `string` | — |  |
| `request_data` | `json` | `{}` |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.obs.set_current_program_scene`

**OBS: Set Current Program Scene** — OBS WebSocket v5 の `SetCurrentProgramScene` を呼び、現在の番組シーンを切り替える。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `scene_name` | `string` | — |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.obs.set_scene_item_enabled`

**OBS: Set Scene Item Enabled** — OBS WebSocket v5 の `SetSceneItemEnabled` を呼び、scene item の表示/非表示を切り替える。`scene_item_id` が負なら `source_name` から ID を解決する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `timeout_ms` | `int` | `3000` |  |
| `scene_name` | `string` | — |  |
| `source_name` | `string` | `""` |  |
| `scene_item_id` | `int` | `-1` |  |
| `enabled` | `bool` | `true` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |
| `scene_item_id` | `int` |  |

### `flowgraph.obs.start_record`

**OBS: Start Record** — OBS WebSocket v5 の `StartRecord` を呼び、録画を開始する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.obs.start_stream`

**OBS: Start Stream** — OBS WebSocket v5 の `StartStream` を呼び、配信を開始する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.obs.stop_record`

**OBS: Stop Record** — OBS WebSocket v5 の `StopRecord` を呼び、録画を停止する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.obs.stop_stream`

**OBS: Stop Stream** — OBS WebSocket v5 の `StopStream` を呼び、配信を停止する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.obs.trigger_studio_mode_transition`

**OBS: Trigger Studio Mode Transition** — OBS WebSocket v5 の `TriggerStudioModeTransition` を呼び、Studio Mode の transition を実行する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `url` | `string` | `"ws://127.0.0.1:4455"` |  |
| `password` | `string` | `""` |  |
| `timeout_ms` | `int` | `3000` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `ok` | `bool` |  |
| `status_code` | `int` |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

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
| `result` | `result<string>` |  |

## osc

### `flowgraph.osc.send`

**OSC: UDP Send** — 単一 OSC メッセージを UDP で送信する。args は JSON 配列（数値・文字列・真偽・null・ネスト配列）

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `path` | `string` | — |  |
| `args` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

## process

### `flowgraph.process.kill`

**Process: Kill** — pid または name_filter に一致する process を終了する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `pid` | `int` | `0` |  |
| `name_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |
| `force` | `bool` | `true` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `killed_count` | `int` |  |
| `pids` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.process.running`

**Process: Running** — pid または name_filter で process の生存状態を確認する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `pid` | `int` | `0` |  |
| `name_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `running` | `bool` |  |
| `count` | `int` |  |
| `pids` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.process.spawn`

**Process: Spawn** — 外部 process を起動する。shell は介さず command + args を直接実行する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `command` | `string` | — |  |
| `args` | `list<string>` | `[]` |  |
| `working_dir` | `string` | `""` |  |
| `env` | `json` | `{}` |  |
| `wait` | `bool` | `false` |  |
| `timeout_ms` | `int` | `0` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `pid` | `int` |  |
| `exit_code` | `int` |  |
| `stdout` | `string` |  |
| `stderr` | `string` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.process.wait`

**Process: Wait** — 指定 pid が終了するまで polling で待つ。exit_code は未取得時 -1。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `pid` | `int` | — |  |
| `timeout_ms` | `int` | `30000` |  |
| `poll_interval_ms` | `int` | `250` |  |

| Output | Type | Note |
|---|---|---|
| `on_exit` | `exec` (out) |  |
| `on_timeout` | `exec` (out) |  |
| `exited` | `bool` |  |
| `exit_code` | `int` |  |
| `error` | `string` |  |
| `result` | `result<bool>` |  |

## random

### `flowgraph.random.normal`

**Random normal** — Gaussian sample (Box–Muller) with given mean and stddev. stddev must be non-negative; 0 yields mean.

| Input | Type | Default | Note |
|---|---|---|---|
| `mean` | `float` | — |  |
| `stddev` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `value` | `float` |  |

### `flowgraph.random.uniform_float`

**Random uniform (float)** — Uniform random float in [lo, hi) half-open interval.

| Input | Type | Default | Note |
|---|---|---|---|
| `lo` | `float` | — |  |
| `hi` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `value` | `float` |  |

### `flowgraph.random.uniform_int`

**Random uniform (int)** — Uniform random integer in [lo, hi] inclusive.

| Input | Type | Default | Note |
|---|---|---|---|
| `lo` | `int` | — |  |
| `hi` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `value` | `int` |  |

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
| `result` | `result<json>` |  |

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

## system

### `flowgraph.system.cpu_usage`

**System: CPU Usage** — sysinfo で全体 CPU 使用率と logical CPU ごとの使用率を取得する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `usage_percent` | `float` |  |
| `core_count` | `int` |  |
| `per_cpu` | `json` |  |

### `flowgraph.system.load_avg`

**System: Load Average** — OS の load average を取得する。未対応OSでは sysinfo の値をそのまま返す。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `one` | `float` |  |
| `five` | `float` |  |
| `fifteen` | `float` |  |

### `flowgraph.system.memory`

**System: Memory** — sysinfo で RAM / swap の使用量を byte 単位で取得する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `mem_used` | `int` |  |
| `mem_total` | `int` |  |
| `mem_available` | `int` |  |
| `mem_usage_percent` | `float` |  |
| `swap_used` | `int` |  |
| `swap_total` | `int` |  |

### `flowgraph.system.process_list`

**System: Process List** — sysinfo で process 一覧を JSON 配列として取得する。name_filter と limit で絞り込める。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `name_filter` | `string` | `""` |  |
| `limit` | `int` | `100` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `count` | `int` |  |
| `processes` | `json` |  |

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
| `result` | `result<table>` |  |

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
| `result` | `result<int>` |  |

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
| `result` | `result<string>` |  |

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
| `result` | `result<string>` |  |

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
| `result` | `result<json>` |  |

## twitch

### `flowgraph.twitch.ad_run`

**Twitch: Run Ad** — Helix POST /channels/commercial で広告を実行する。length_seconds は通常 30/60/90/120/150/180。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `length_seconds` | `int` | `60` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

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

### `flowgraph.twitch.channel_info_update`

**Twitch: Update Channel Info** — Helix PATCH /channels で配信タイトル、カテゴリ、言語、タグを更新する。空入力は送信しない。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `game_id` | `string` | `""` |  |
| `title` | `string` | `""` |  |
| `broadcaster_language` | `string` | `""` |  |
| `tags` | `list<string>` | `[]` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.twitch.chat_clear`

**Twitch: Clear Chat** — Helix DELETE /moderation/chat でチャット全体または指定messageを削除する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `moderator_id` | `string` | — |  |
| `message_id` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

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

### `flowgraph.twitch.chat_settings_update`

**Twitch: Update Chat Settings** — Helix PATCH /chat/settings で emote/subscriber/follower/slow/unique chat などの設定を更新する。接続された入力だけ送信する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `moderator_id` | `string` | — |  |
| `emote_mode` | `bool` | — |  |
| `subscriber_mode` | `bool` | — |  |
| `unique_chat_mode` | `bool` | — |  |
| `follower_mode` | `bool` | — |  |
| `follower_mode_duration` | `int` | — |  |
| `slow_mode` | `bool` | — |  |
| `slow_mode_wait_time` | `int` | — |  |
| `non_moderator_chat_delay` | `bool` | — |  |
| `non_moderator_chat_delay_duration` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.twitch.clip_create`

**Twitch: Create Clip** — Helix POST /clips で現在の配信からclip作成を開始する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `has_delay` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |
| `clip_id` | `string` |  |
| `edit_url` | `string` |  |

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

### `flowgraph.twitch.goals_get`

**Twitch: Get Goals** — Helix GET /goals でチャンネルのcreator goals一覧を取得する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |
| `goals` | `json` |  |

### `flowgraph.twitch.poll_create`

**Twitch: Create Poll** — Helix POST /polls でpollを作成する。choices は文字列配列または {title} 配列。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `title` | `string` | — |  |
| `choices` | `json` | — |  |
| `duration_seconds` | `int` | `60` |  |
| `channel_points_voting_enabled` | `bool` | `false` |  |
| `channel_points_per_vote` | `int` | `0` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |
| `poll_id` | `string` |  |

### `flowgraph.twitch.poll_end`

**Twitch: End Poll** — Helix PATCH /polls でpollを終了する。status は TERMINATED または ARCHIVED。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `poll_id` | `string` | — |  |
| `status` | `string` | `"TERMINATED"` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.twitch.prediction_create`

**Twitch: Create Prediction** — Helix POST /predictions でChannel Points predictionを作成する。outcomes は文字列配列または {title} 配列。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `title` | `string` | — |  |
| `outcomes` | `json` | — |  |
| `prediction_window_seconds` | `int` | `120` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |
| `prediction_id` | `string` |  |

### `flowgraph.twitch.prediction_end`

**Twitch: End Prediction** — Helix PATCH /predictions でpredictionを LOCKED / RESOLVED / CANCELED にする。RESOLVED は winning_outcome_id 必須。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `prediction_id` | `string` | — |  |
| `status` | `string` | `"CANCELED"` |  |
| `winning_outcome_id` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.twitch.raid_cancel`

**Twitch: Cancel Raid** — Helix DELETE /raids で保留中の raid をキャンセルする。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.twitch.raid_start`

**Twitch: Start Raid** — Helix POST /raids で raid を開始する。実際のraidはTwitch側の90秒カウントダウン後に行われる。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `to_broadcaster_id` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.twitch.shield_mode_update`

**Twitch: Update Shield Mode** — Helix PUT /moderation/shield_mode で Shield Mode を有効化/無効化する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `moderator_id` | `string` | — |  |
| `is_active` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |

### `flowgraph.twitch.stream_marker_create`

**Twitch: Create Stream Marker** — Helix POST /streams/markers で配信マーカーを追加する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `broadcaster_id` | `string` | — |  |
| `access_token` | `string` | — |  |
| `client_id` | `string` | — |  |
| `endpoint` | `string` | `""` |  |
| `description` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `response` | `json` |  |
| `error` | `string` |  |
| `result` | `result<json>` |  |
| `marker_id` | `string` |  |

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

### `flowgraph.unit.try_parse`

**Unit Try Parse** — Parse a quantity string and return failure as result<quantity> instead of halting.

| Input | Type | Default | Note |
|---|---|---|---|
| `text` | `string` | — |  |

| Output | Type | Note |
|---|---|---|
| `ok` | `bool` |  |
| `quantity` | `quantity` |  |
| `error` | `string` |  |
| `result` | `result<quantity>` |  |

## util

### `flowgraph.util.debounce`

**Debounce** — After `value` JSON stops changing for `deadtime_ms`, updates `value_out` to the stable value. Each change restarts the timer (`ctx.trigger` + `__resume__`). Requires `run_forever` for async debounce.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `json` | `null` |  |
| `deadtime_ms` | `int` | `100` |  |
| `__resume__` | `exec` (in) | — |  |
| `__pending_id__` | `int` | `-1` |  |

| Output | Type | Note |
|---|---|---|
| `value_out` | `json` |  |

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

### `flowgraph.util.edge_detect`

**Edge detect** — On each `exec_in`, compares `value` to the previous sample and may fire `on_edge` with `edge_type` (`rising` / `falling`). Property `mode`: `rising` | `falling` | `both` (default). Pull-only reads return the last `edge_type` string (initially empty). Wire `exec_in` together with the signal you sample (e.g. `state.bool` `changed` exec).

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `value` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `edge_type` | `string` |  |
| `on_edge` | `exec` (out) |  |

| Property | Type | Default | Required | Note |
|---|---|---|---|---|
| `mode` | `string` | `"both"` |  | rising: low→high only, falling: high→low only, both: either transition. |

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

### `flowgraph.util.prev_value`

**Previous value** — Each time `value` is evaluated, outputs `prev`: the **previous** input JSON. First sample: `prev` equals the current `value`.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `prev` | `json` |  |

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

### `flowgraph.util.sample_hold`

**Sample & hold** — While `sample_exec` fires, captures the current `value` JSON into `held`. Between samples, `held` stays constant.

| Input | Type | Default | Note |
|---|---|---|---|
| `sample_exec` | `exec` (in) | — |  |
| `value` | `json` | `null` |  |

| Output | Type | Note |
|---|---|---|
| `held` | `json` |  |

### `flowgraph.util.throttle`

**Throttle** — Leading-edge throttle on JSON `value`: when the value **differs** from the last emitted one, emit immediately only if at least `interval_ms` has passed since the last emit; otherwise drop the update. Identical consecutive values are passed through without resetting the timer.

| Input | Type | Default | Note |
|---|---|---|---|
| `value` | `json` | `null` |  |
| `interval_ms` | `int` | `100` |  |

| Output | Type | Note |
|---|---|---|
| `value_out` | `json` |  |

### `flowgraph.util.timer_interval`

**Timer Interval** — Periodic timer: while `enabled` and `run_forever` trigger bus is active, fires `on_tick` every `interval_sec` (min 0.01s, sleep min 10ms). Outputs `count` (total ticks) and `elapsed_sec` (wall time since previous tick, or `interval_sec` on first tick). Stale wakeups are dropped. `execute()` one-shot mode does not arm (same as `util.delay`).

| Input | Type | Default | Note |
|---|---|---|---|
| `enabled` | `bool` | `true` |  |
| `interval_sec` | `float` | `1.0` |  |
| `__tick__` | `exec` (in) | — |  |
| `__pending_id__` | `int` | `-1` |  |

| Output | Type | Note |
|---|---|---|
| `on_tick` | `exec` (out) |  |
| `count` | `int` |  |
| `elapsed_sec` | `float` |  |

## vec

### `flowgraph.vec2.add`

**Vec2 add** — Componentwise vec2 addition

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec2.distance`

**Vec2 distance** — Euclidean distance between two vec2s

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.vec2.dot`

**Vec2 dot** — Vec2 dot product (returns scalar)

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.vec2.length`

**Vec2 length** — Euclidean magnitude of a vec2: sqrt(x^2 + y^2)

| Input | Type | Default | Note |
|---|---|---|---|
| `v` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.vec2.lerp`

**Vec2 lerp** — Componentwise linear interpolation: a + (b - a) * t for 2-vectors. t is not clamped.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |
| `t` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec2.make`

**Vec2 make** — Pack 2 floats into a JSON array [x, y]

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `float` | — |  |
| `y` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `v` | `json` |  |

### `flowgraph.vec2.normalize`

**Vec2 normalize** — Scale a vec2 to unit length. Zero vector returns [0, 0] (not an error).

| Input | Type | Default | Note |
|---|---|---|---|
| `v` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec2.scale`

**Vec2 scale** — Multiply every component of a 2-vector by scalar `k`

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `k` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec2.sub`

**Vec2 sub** — Componentwise vec2 subtraction

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec2.unpack`

**Vec2 unpack** — Unpack a 2-component JSON array into separate Float outputs

| Input | Type | Default | Note |
|---|---|---|---|
| `v` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `x` | `float` |  |
| `y` | `float` |  |

### `flowgraph.vec3.add`

**Vec3 add** — Componentwise vec3 addition

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec3.distance`

**Vec3 distance** — Euclidean distance between two vec3s

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.vec3.dot`

**Vec3 dot** — Vec3 dot product (returns scalar)

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.vec3.length`

**Vec3 length** — Euclidean magnitude of a vec3: sqrt(x^2 + y^2 + z^2)

| Input | Type | Default | Note |
|---|---|---|---|
| `v` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `float` |  |

### `flowgraph.vec3.lerp`

**Vec3 lerp** — Componentwise linear interpolation: a + (b - a) * t for 3-vectors. t is not clamped.

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |
| `t` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec3.make`

**Vec3 make** — Pack 3 floats into a JSON array [x, y, z]

| Input | Type | Default | Note |
|---|---|---|---|
| `x` | `float` | — |  |
| `y` | `float` | — |  |
| `z` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `v` | `json` |  |

### `flowgraph.vec3.normalize`

**Vec3 normalize** — Scale a vec3 to unit length. Zero vector returns [0, 0, 0] (not an error).

| Input | Type | Default | Note |
|---|---|---|---|
| `v` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec3.scale`

**Vec3 scale** — Multiply every component of a 3-vector by scalar `k`

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `k` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec3.sub`

**Vec3 sub** — Componentwise vec3 subtraction

| Input | Type | Default | Note |
|---|---|---|---|
| `a` | `json` | — |  |
| `b` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `result` | `json` |  |

### `flowgraph.vec3.unpack`

**Vec3 unpack** — Unpack a 3-component JSON array into separate Float outputs

| Input | Type | Default | Note |
|---|---|---|---|
| `v` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `x` | `float` |  |
| `y` | `float` |  |
| `z` | `float` |  |

## vmc

### `flowgraph.vmc.extract_blendshape`

**VMC: Extract BlendShape** — `MotionFrame` から `/VMC/Ext/Blend/Val` を探し、JSON と型付きの `found/name/value` を返す。無ければ JSON は null、typed output は既定値。

| Input | Type | Default | Note |
|---|---|---|---|
| `frame` | `motion_frame` | — |  |
| `blendshape_name` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `blendshape` | `json` |  |
| `found` | `bool` |  |
| `name` | `string` |  |
| `value` | `float` |  |

### `flowgraph.vmc.extract_bone_pos`

**VMC: Extract Bone Pos** — `MotionFrame` から `/VMC/Ext/Bone/Pos` を探し、JSON と型付きの `found/bone/px/py/pz/rx/ry/rz/rw` を返す。無ければ JSON は null、typed output は既定値。

| Input | Type | Default | Note |
|---|---|---|---|
| `frame` | `motion_frame` | — |  |
| `bone_name` | `string` | `""` |  |

| Output | Type | Note |
|---|---|---|
| `pose` | `json` |  |
| `found` | `bool` |  |
| `bone` | `string` |  |
| `px` | `float` |  |
| `py` | `float` |  |
| `pz` | `float` |  |
| `rx` | `float` |  |
| `ry` | `float` |  |
| `rz` | `float` |  |
| `rw` | `float` |  |

### `flowgraph.vmc.extract_root_pos`

**VMC: Extract Root Pos** — `MotionFrame` から `/VMC/Ext/Root/Pos` を探し、JSON と型付きの `found/bone/px/py/pz/rx/ry/rz/rw` を返す。無ければ JSON は null、typed output は既定値。

| Input | Type | Default | Note |
|---|---|---|---|
| `frame` | `motion_frame` | — |  |

| Output | Type | Note |
|---|---|---|
| `pose` | `json` |  |
| `found` | `bool` |  |
| `bone` | `string` |  |
| `px` | `float` |  |
| `py` | `float` |  |
| `pz` | `float` |  |
| `rx` | `float` |  |
| `ry` | `float` |  |
| `rz` | `float` |  |
| `rw` | `float` |  |

### `flowgraph.vmc.send_bone_pos`

**VMC: Send Bone Pos** — VMC `/VMC/Ext/Bone/Pos` を 1 回 UDP 送信。`position` は [x,y,z]、`rotation` は [qx,qy,qz,qw] の JSON 配列

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `bone_name` | `string` | — |  |
| `position` | `json` | — |  |
| `rotation` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.vmc.send_root_pos`

**VMC: Send Root Pos** — VMC `/VMC/Ext/Root/Pos` を 1 回 UDP 送信（骨名は常に `root`）。`position` / `rotation` は JSON 配列

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `position` | `json` | — |  |
| `rotation` | `json` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

## vrchat

### `flowgraph.vrchat.avatar_parameter_bool`

**VRChat: Avatar Parameter (Bool)** — OSC `/avatar/parameters/<name>` に bool を 1 つ送信

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `parameter_name` | `string` | — |  |
| `value` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.vrchat.avatar_parameter_float`

**VRChat: Avatar Parameter (Float)** — OSC `/avatar/parameters/<name>` に float を 1 つ送信（VRChat OSC Avatar Parameters）

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `parameter_name` | `string` | — |  |
| `value` | `float` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.vrchat.avatar_parameter_int`

**VRChat: Avatar Parameter (Int)** — OSC `/avatar/parameters/<name>` に int を 1 つ送信

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `parameter_name` | `string` | — |  |
| `value` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.vrchat.chatbox_input`

**VRChat: Chatbox Input** — `/chatbox/input` に (text, send_immediately, play_notification_sfx)。テキストは最大 144 文字に切り詰め

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `text` | `string` | — |  |
| `send_immediately` | `bool` | `true` |  |
| `play_notification_sfx` | `bool` | `true` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.vrchat.chatbox_typing`

**VRChat: Chatbox Typing** — `/chatbox/typing` に bool を 1 つ送信

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `host` | `string` | — |  |
| `port` | `int` | — |  |
| `typing` | `bool` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `bytes_sent` | `int` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

## window

### `flowgraph.window.close`

**Window: Close** — 対象ウィンドウへ WM_CLOSE を送る。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.enum`

**Window: Enum** — トップレベルウィンドウ一覧を Table として取得する。Windows first。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |
| `visible_only` | `bool` | `true` |  |

| Output | Type | Note |
|---|---|---|
| `exec_out` | `exec` (out) |  |
| `windows` | `table` |  |
| `count` | `int` |  |
| `error` | `string` |  |
| `result` | `result<table>` |  |

### `flowgraph.window.foreground`

**Window: Foreground** — 対象ウィンドウを前面化する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.maximize`

**Window: Maximize** — 対象ウィンドウを最大化する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.minimize`

**Window: Minimize** — 対象ウィンドウを最小化する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.move`

**Window: Move** — 対象ウィンドウの左上座標を変更する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |
| `x` | `int` | — |  |
| `y` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.pseudo_fullscreen`

**Window: Pseudo Fullscreen** — 対象ウィンドウの表示状態を保存して最大化する first slice。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.pseudo_fullscreen_exit`

**Window: Pseudo Fullscreen Exit** — pseudo_fullscreen 前の表示状態を復元する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.resize`

**Window: Resize** — 対象ウィンドウのサイズを変更する。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |
| `width` | `int` | — |  |
| `height` | `int` | — |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

### `flowgraph.window.restore`

**Window: Restore** — 対象ウィンドウを通常表示へ戻す。

| Input | Type | Default | Note |
|---|---|---|---|
| `exec_in` | `exec` (in) | — |  |
| `hwnd` | `int` | `0` |  |
| `pid` | `int` | `0` |  |
| `title_filter` | `string` | `""` |  |
| `exact` | `bool` | `false` |  |

| Output | Type | Note |
|---|---|---|
| `on_success` | `exec` (out) |  |
| `on_error` | `exec` (out) |  |
| `affected_count` | `int` |  |
| `windows` | `table` |  |
| `error` | `string` |  |
| `result` | `result<int>` |  |

