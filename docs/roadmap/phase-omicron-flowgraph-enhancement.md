# Phase ο — Flowgraph Enhancement I (計算系 + 時間 + signal util + GUI 小改善)

> **Status**: ο-0 docs / ο-1 math (42 ノード) / ο-2 easing / ο-3 vec / **ο-4 `timer_interval`** 着地済み。ο-5 以降は Phase ξ-5 (GUI) と並行進行可。
> 起点となるスコープ感は [`../roadmap.md`](../roadmap.md) の "Phase ο" を参照。依存する単位次元基盤は [`phase-ksi-dimensional-quantity-system.md`](phase-ksi-dimensional-quantity-system.md)。

---

## 0. Status

- Phase ν-β クローズ後の次フェーズ。[docs/roadmap.md](../roadmap.md) の "Flowgraph 機能向上（TBD）" を 1 つの narrow フェーズとして具体化する。
- 本フェーズは **engine 内完結の Pure ノード群 + GUI 小改善** に閉じる。外部連携（HTTP / OBS / OSC / VMC / system metrics / process / window）と engine の大改修（Undo/Redo / subgraph / 物理）は明示的に次フェーズ（Phase π で DateTime 基盤、続く ρ / σ / τ / υ / ω が旧 π〜υ を 1 文字繰り下げ）に送る（§4）。
- 全 6 カテゴリ（math / easing / vec / time / signal util / random）× **78 ノード**（math 42 + easing 1 + vec 20 + time 5 + signal util 5 + random/noise 5）+ GUI 3 項目で構成。1 サブフェーズ = 1 commit 粒度に分解（§6）。ο-0 docs では vec を 18 と記していたが、ο-3 実装時に行数と実ノード数のずれ（`add/sub` を 1 行で書いていた）を正確化して 20 に修正（math の 37 → 42 修正と同種）。
- **依存**: [Phase ξ (Dimensional Quantity System)](phase-ksi-dimensional-quantity-system.md) 先行。ο-1 math / ο-2 easing / ο-3 vec / ο-4 time / ο-5 signal util のノード群は最初から `Quantity<Dimension>` 対応で実装する。ξ-0 / ξ-1 / ξ-2 / ξ-3 着地前に ο-1 を走らせると retrofit 地獄になるため、**順序ブロッカー**として固定する。

---

## 1. 動機

現行 `flowgraph.math.*` は `int_add / int_sub / int_mul / int_div / int_mod` + `float_add / float_sub / float_mul / float_div` の **9 ノードだけ**（[`src/flowgraph/nodes/math.rs`](../../src/flowgraph/nodes/math.rs)）。以下のユースケースで毎回「外部 util に漏らす」か「複数ノードで手組みする」羽目になり、Flowgraph 上の表現力が頭打ちになっている。

| 領域 | 具体例 | 現状 |
|---|---|---|
| アバター表情補間 | 口パク開閉度を [0, 1] に clamp → easing で整形 → VMC 送信 | clamp / easing ノードが無く、flow 側で二項算術の組合せで擬似再現 |
| 数値演算全般 | 音量 dB → 線形振幅、RMS / ピーク算出、角度 ↔ ラジアン | sqrt / pow / log / exp / trig / deg↔rad ノードが無い |
| procedural motion | 「首が Perlin ノイズで微小に揺れる」「呼吸でスケールが sin 波」 | noise / time / sin が無く source が無いので組めない |
| 周期処理 | N 秒おきに state をチェック、デバッグ用に定期 tick | **`flowgraph.util.timer_interval`**（ο-4、`timer_interval.rs` + `DelayNode` 系 self-trigger） |
| 入力デバウンス | ボタン連打 / 音声検出の立ち上がりだけで発火したい | `rate_limit` は「量」制御で、`edge_detect` / `debounce`（値通過 + deadtime）は別モノなのに代替手段がない |
| GUI editor | ノードを「ここに」追加したい / category を一括で隠したい / 選択ノードを複製したい | palette クリック追加は盤面中央 + 乱数、複製は TOML fragment copy-paste 経由 |

これらは **engine 内完結 + 外部 IO ゼロ** で片付くため、φ / γ / ν のような protocol / infra フェーズと分離して安全に追加できる。

ν-β で ν 系 E2E 基盤が揃い、回帰検知ができる状態になったので、ノード拡充フェーズを走らせるのに **都合が良いタイミング** でもある。

---

## 2. スコープ方針

**narrow-scoped**。本フェーズのゴールは「Flowgraph で日常的に欲しい計算・時間・signal util を一通り揃え、editor も小物 UX を上積みする」まで。

- 追加依存は最小限: 新規 crate は `noise` 1 件のみ候補（§3.6）、他は既存 (`chrono` / `rand` / `std`) で賄う。
- 既存ノード動作は変更しない。`flowgraph.math.*` は完全後方互換（new node 追加のみ）。
- GUI 改善は `FlowgraphPalette.svelte` / `FlowgraphCanvas.svelte` / `FlowgraphTab.svelte` に閉じ、他タブには波及させない。
- Breaking change を作らない（`CHANGELOG.md` の `### Breaking changes (ο)` が空で終わる状態を狙う）。
- **Phase ξ 依存**: ο-1〜ο-5 のノードは **最初から `Quantity<Dimension>` 対応**で書く（ο を先に plain-float で実装して ξ-3 で全数 retrofit する案は却下、理由は§5.0）。trig / arctrig / normalize_angle は Angle 次元を、sinh/cosh/tanh は dimensionless を型制約として要求する。vec2/vec3 の成分は同一 Dimension を持つ必要があり、`flowgraph.vec2.add` などは両辺の次元一致を engine error で担保する。

以下は **非スコープ**（明示的に §4 で次フェーズへ送る）:

- Stateful の engine ロジック拡張（undo / subgraph / re-entrancy）
- 外部プロトコル（OSC / VMC / VRC / OBS / HTTP / Discord voice / VTS / MIDI / Global hotkey）
- OS リソース（process / window / system metrics / GPU）
- 音声合成・再生（SE / envelope / pitch）
- 物理（spring / damper / integrator / gravity）

---

## 3. 新ノード・機能一覧

### 3.1 math 拡張 (`flowgraph.math.*`)

全 Pure。既存 `src/flowgraph/nodes/math.rs` を拡張し、型分離ポリシー（int 版 / float 版）を維持する。NaN / ±Inf の扱いは既存 `float_div` と揃え、**エラーにはしない**で値として伝搬（下流 `flowgraph.json.stringify` 経由で観測可能）。

| feature | 型 | 入出力 | 備考 |
|---|---|---|---|
| `flowgraph.math.abs_int` | Int → Int | `x` → `result` | overflow（`i64::MIN`）は `wrapping_abs` |
| `flowgraph.math.abs_float` | Float → Float | `x` → `result` | — |
| `flowgraph.math.min_int` / `.max_int` | (Int, Int) → Int | `a, b` → `result` | — |
| `flowgraph.math.min_float` / `.max_float` | (Float, Float) → Float | `a, b` → `result` | NaN は `b` 優先（`f64::min` セマンティクス）|
| `flowgraph.math.clamp_int` / `.clamp_float` | (X, X, X) → X | `value, lo, hi` → `result` | `lo > hi` は swap して吸収（spec 中で明記）|
| `flowgraph.math.lerp` | (Float, Float, Float) → Float | `a, b, t` → `result` | `t` は clamp しない（外挿許容）|
| `flowgraph.math.inverse_lerp` | (Float, Float, Float) → Float | `a, b, v` → `result` | `a == b` は 0.0 を返す |
| `flowgraph.math.remap` | (Float × 5) → Float | `value, in_lo, in_hi, out_lo, out_hi` → `result` | `in_lo == in_hi` は `out_lo` |
| `flowgraph.math.smoothstep` | (Float × 3) → Float | `edge0, edge1, x` → `result` | GLSL 準拠 |
| `flowgraph.math.sin` / `.cos` / `.tan` | Float → Float | ラジアン入力。Phase ξ 着地後は **Angle 次元必須** |
| `flowgraph.math.asin` / `.acos` / `.atan` | Float → Float | Phase ξ 着地後は **Angle 次元を返す** |
| `flowgraph.math.atan2` | (Float, Float) → Float | `y, x` → `result`。Phase ξ 着地後は Angle 次元を返す |
| `flowgraph.math.sinh` / `.cosh` / `.tanh` | Float → Float | 双曲線関数。`f64::sinh` / `.cosh` / `.tanh` ラップ。Phase ξ では **dimensionless のみ受ける**（双曲線関数の数学的文脈では独立変数は無次元）|
| `flowgraph.math.asinh` / `.acosh` / `.atanh` | Float → Float | 逆双曲線関数。`.acosh` は `x < 1` で NaN、`.atanh` は `|x| >= 1` で NaN（std::f64 準拠）|
| `flowgraph.math.sqrt` | Quantity → Quantity | `x` → `result` | **次元対応**: `sqrt(m²) = m`、`sqrt(m²/s²) = m/s`。全 atom exponent が偶数の場合に限り次元 sqrt を返す（`Quantity::try_sqrt` 委譲）。奇数 exponent（`sqrt(m)`）は integer-dimension 型システムの制約で error。絶対温度 K は禁止。負値は NaN（std::f64 準拠）。Phase ο-1.1 で dimensionless-only から昇格 |
| `flowgraph.math.pow` | (Float, Float) → Float | `base, exp` | |
| `flowgraph.math.exp` / `.ln` / `.log2` / `.log10` | Float → Float | | |
| `flowgraph.math.sign_int` / `.sign_float` | X → X | -1 / 0 / 1 | float は NaN で 0 |
| `flowgraph.math.floor` / `.ceil` / `.round` | Float → Float | `round` は half-away-from-zero（std 既定）| |
| `flowgraph.math.deg_to_rad` / `.rad_to_deg` | Float → Float | 数値としての scale 変換。Phase ξ 着地後は `flowgraph.unit.convert` と共存（単位変換の明示 UI が ο-1 時点では `deg_to_rad` / `rad_to_deg` のみ提供）|
| `flowgraph.math.normalize_angle_deg_0_360` | Float → Float | `1357.33` → `277.33`（`x.rem_euclid(360.0)`）。`[0, 360)` に正規化。Phase ξ 着地後は **Angle 次元 + deg 単位必須** |
| `flowgraph.math.normalize_angle_deg_signed` | Float → Float | `277.33` → `-82.67`（`((x + 180) mod 360) - 180`）。`[-180, +180)` に正規化 |
| `flowgraph.math.normalize_angle_rad_0_2pi` | Float → Float | `x.rem_euclid(2π)`。`[0, 2π)` に正規化 |
| `flowgraph.math.normalize_angle_rad_signed` | Float → Float | `[-π, +π)` に正規化 |

計 **42 ノード**（2026-04-24 拡張: 双曲線 6 + 角度正規化 4、ο-1 着手時に表の行を素直に列挙して 42 に正確化。ο-0 docs での「37」表記は誤記）。既存 `int_binop_node!` / `float_binop_node!` マクロを参考に、1 入力 / 3 入力系のマクロを増設する方針。

> **Note (Phase ξ 依存)**: 本フェーズのこれらの math ノードは **Phase ξ (Dimensional Quantity System) 着地後の ο-1 着手**が前提。ο-1 以降の実装では最初から `Quantity<Dimension>` を受ける形で書き、Phase ξ が提供する Angle 次元 + rad/deg unit を trig / arctrig / normalize_angle に型制約として載せる。sinh/cosh/tanh 系は **dimensionless のみ**（双曲線関数の引数に物理単位を持たせると SI 上の意味を失う）。`deg_to_rad` / `rad_to_deg` は ξ 着地後は単に `flowgraph.unit.convert{to: "rad"}` / `{to: "deg"}` の薄いラッパーに退化する可能性があり、deprecation policy を ξ-3 retrofit 時に再整理する。

### 3.2 easing (`flowgraph.easing.*`)

全 Pure。`t: Float ∈ [0, 1]` を入れると補間値 `[0, 1]`（またはモードによっては外に出る）を返す。

設計判断（§5.2 で詳述）: **`flowgraph.easing.apply` 1 ノードに `curve` プロパティ（enum）を持たせる** 方式を採用する。理由:

- 個別ノード化（`cubic_in` / `cubic_out` / …）だと 6 family × 3 mode = **18 ノード** で palette が肥大化する。
- GUI 側の「実行時に curve を切り替えたい」ユースケース（debug / A/B テスト）に対して、`apply` を共有しておけば property を書き換えるだけで済む。
- curve 追加は enum のバリアント追加 = 1 関数追加で済む（ノード追加より低コスト）。

| feature | 仕様 |
|---|---|
| `flowgraph.easing.apply` | 入力 `t: Float`、プロパティ `curve` (enum: `linear` / `quad_in` / `quad_out` / `quad_inout` / `cubic_in` / `cubic_out` / `cubic_inout` / `sine_in` / `sine_out` / `sine_inout` / `expo_in` / `expo_out` / `expo_inout` / `elastic_in` / `elastic_out` / `elastic_inout` / `bounce_in` / `bounce_out` / `bounce_inout`) + `clamp_t: bool = true`、出力 `value: Float` |

オプション補助として `flowgraph.easing.remap`（`lerp + easing.apply + remap` を 1 ノードで済ませる糖衣）も候補だが、ο-2 では必須ではないので risk セクション扱い。

### 3.3 vector (`flowgraph.vec2.*` / `flowgraph.vec3.*`)

全 Pure。値は **`SocketType::Json`（既存）上で `[x, y]` / `[x, y, z]` の配列リテラル**として表現する。独自 `SocketType::Vec2/Vec3` を導入しない理由:

- 型システムに新 variant を追加すると `to_socket_value` / serialization / port visual（GUI 側 `FlowgraphNodeCard.svelte`）すべてに波及し、narrow scope を超える。
- `List<Float>` では「要素数が保証された 2 / 3 次元ベクトル」という **意味**を乗せられず、GUI 表示も冗長。
- `Json` なら JSON 配列リテラルで書けて、後続の OSC / VMC phase（π）でそのまま wire format にマップできる。

| feature | 入出力 | 備考 |
|---|---|---|
| `flowgraph.vec2.make` | (Float, Float) → Json | `x, y` → `[x, y]` |
| `flowgraph.vec2.unpack` | Json → (Float, Float) | `v` → `x, y` |
| `flowgraph.vec2.add` / `.sub` | (Json, Json) → Json | 成分ごと |
| `flowgraph.vec2.scale` | (Json, Float) → Json | |
| `flowgraph.vec2.dot` | (Json, Json) → Float | |
| `flowgraph.vec2.length` | Json → Float | |
| `flowgraph.vec2.normalize` | Json → Json | 零ベクトルは `[0, 0]` |
| `flowgraph.vec2.lerp` | (Json, Json, Float) → Json | `a, b, t` |
| `flowgraph.vec2.distance` | (Json, Json) → Float | |
| `flowgraph.vec3.*` | 上記 9 ノードの 3 次元版 | |

計 20 ノード（vec2 10 + vec3 10。ο-3 実装時に `add / sub` を 1 行で書いていた行数 9 が実ノード数 10 と食い違っていた点を正確化）。内部で `[f64; N]` に取り出してから計算し、JSON 配列に戻す。不正な配列（要素数不足、非数値、非有限値）は `NodeExecError::Generic` で即停止（既存 json_ops と同じポリシー）。

### 3.4 timer + DateTime（`flowgraph.util.*` / Phase π `flowgraph.datetime.*`）

**Phase π 着地後（π-5）の整理**: 旧案の **`flowgraph.time.*` 4 種**（`now_rfc3339` / `now_epoch_ms` / `format` / `since_ms`）は **ο-4 では実装せず**、型安全な [`flowgraph.datetime.*`](../../src/flowgraph/nodes/datetime.rs) **8 ノード** + `SocketType::DateTime` + `Quantity<time>` に置換（[`phase-pi-datetime-system.md`](phase-pi-datetime-system.md) §4.9、利用者: [`../manual/datetime-system.md`](../manual/datetime-system.md)）。置換の例:

| 旧案（String / Int 中心） | 置換（π-5 以降） |
|---|---|
| `now_rfc3339` | `datetime.now` → `datetime.format`（`rfc3339`） |
| `now_epoch_ms` | `datetime.now` → `datetime.epoch_ms` |
| `format(epoch, fmt)` | `from_epoch_ms` → `format`（`custom` 等） |
| `since_ms(t0)` | `datetime.now` + `diff` + 必要に応じ `flowgraph.unit.convert`（s→ms） |

- **`flowgraph.util.timer_interval`**: **実装済み（ο-4）**。Stateful + `ctx.trigger` 自己 re-arm（`DelayNode` パターン）。lazy graph の初回 bootstrap のため `feature == flowgraph.util.timer_interval` は engine の `sources` 判定で例外扱い。仕様の正本は [backlog-nodes.md §1](backlog-nodes.md) + 下記 §5.4。

### 3.5 signal util (`flowgraph.util.*`)

Stateful。値系の「前値」「立ち上がり」「保持」「連打抑制」を 1 ノード 1 責務で揃える。

| feature | 種別 | 仕様 |
|---|---|---|
| `flowgraph.util.edge_detect` | Stateful | 入力 `value: Bool`、プロパティ `mode: rising/falling/both`。前値と比較して変化時のみ `on_edge: Exec` 発火 + `edge_type: String`（`rising` / `falling`）出力 |
| `flowgraph.util.prev_value` | Stateful | 入力 `value: Json`、出力 `prev: Json`（初回は `current` と同値） |
| `flowgraph.util.sample_hold` | Stateful | 入力 `value: Json` + `sample_exec: Exec`、出力 `held: Json`。`sample_exec` 発火時だけ値を更新 |
| `flowgraph.util.debounce` | Stateful | 入力 `value: Json` + `deadtime_ms: Int`、出力 `value_out: Json`。値が変化してから `deadtime_ms` 経過せず新しい変化が来たらリセット。`ctx.trigger` 経路 |
| `flowgraph.util.throttle` | Stateful | 入力 `value: Json` + `interval_ms: Int`、出力 `value_out: Json`。最低 `interval_ms` に 1 回のペースで下流に流す |

既存 `flowgraph.util.rate_limit` は **exec トークンバケット**（gate）で、本群は **値パス**が責務。命名が近いが役割分担を §5.3 で明記する。

### 3.6 random / noise (`flowgraph.random.*` / `flowgraph.noise.*`)

- `flowgraph.random.uniform_int`: `(lo: Int, hi: Int) → Int`、`[lo, hi]` 閉区間。
- `flowgraph.random.uniform_float`: `(lo: Float, hi: Float) → Float`、`[lo, hi)` 半開区間。
- `flowgraph.random.normal`: `(mean: Float, stddev: Float) → Float`、Box-Muller。
- `flowgraph.noise.perlin_1d`: `(t: Float, seed: Int) → Float ∈ [-1, 1]`。
- `flowgraph.noise.perlin_2d`: `(x: Float, y: Float, seed: Int) → Float ∈ [-1, 1]`。

random 系は `rand::thread_rng()`（すでに推移依存で入ってる可能性高、Cargo.toml 確認事項）、noise 系は **`noise` crate を新規依存として追加**（§7 Risks）。

どちらも「呼び出すたびに値が変わる」＝ Pure 定義を緩めるが、Flowgraph の Pure は「副作用なし」であって「冪等」は要求していないため（既存 `flowgraph.state.*` も状態を持つ Stateful で区別済み）、**Stateful 扱い**にして seed / 内部 RNG state を持たせる方が将来的な reproducibility に寄与する。初期実装は Pure + `thread_rng` で出し、ο+ で Stateful 化する余地を残す（doc に TBD）。

### 3.7 GUI 小改善

すべて既存コンポーネント内部の追加で済み、スキーマ / API 変更なし。

1. **palette カテゴリ絞り込み**: [`FlowgraphPalette.svelte`](../../gui/src/lib/flowgraph/FlowgraphPalette.svelte) に `Set<string> hiddenCategories`（localStorage 永続化）を追加。グループヘッダに toggle ボタン。`grouped` derived を hiddenCategories で濾す。
2. **canvas drop-at-cursor**: [`FlowgraphCanvas.svelte`](../../gui/src/lib/flowgraph/FlowgraphCanvas.svelte) に `ondragover`（`preventDefault`）と `ondrop` を追加。Palette 側は `dragstart` で feature 名を `dataTransfer` に入れる（既に `draggable="true"` はある）。drop 位置は `@xyflow/svelte` の `screenToFlowPosition` で flow 座標に変換。既存 `flowgraphStore.addNode` と `FlowgraphPalette.onAdd` を共通化。
3. **Ctrl+D duplicate**: [`FlowgraphTab.svelte`](../../gui/src/lib/tabs/FlowgraphTab.svelte) の `window keydown` handler に `Ctrl+D` / `Cmd+D` を追加。選択中 1 ノード（`flowgraphStore.selectedNodeId`）の feature / properties をコピーし、`uniqueId` で新 id を発番、position を `(+24, +24)` オフセットして `addNode` → 新 id を selection に移す。ブラウザ既定の "bookmark this tab" を `ev.preventDefault()` でキャンセルする点に注意。

ν-β-2 の [`gui/tests/e2e/flowgraph-canvas-basic.spec.ts`](../../gui/tests/e2e/flowgraph-canvas-basic.spec.ts) に assertion を増量して 3 項目の回帰検知を追加する（ο-6 の作業）。

---

## 4. 非スコープ（明示的に次フェーズへ）

| 項目 | 送り先 | 理由 |
|---|---|---|
| OSC send/recv / VMC / VRC / iFacialMocap | **Phase ρ** | UDP socket 管理・protocol 仕様依存があり、engine 外の非同期ランタイム / conf スキーマ拡張も絡む（旧 Phase π、π は DateTime 基盤に割当られたため 1 文字繰り下げ） |
| 汎用 HTTP (`flowgraph.http.request`) / OBS WebSocket / system metrics (`sysinfo` / NVML) / Twitch Helix 拡張（RAID / Ad / chat settings / prediction / poll / shield / marker / clip / channel info / goals / chat clear） | **Phase σ** | 外部 API + auth + retry + timeout の横串、および既存 `[src/flowgraph/nodes/twitch.rs](../../src/flowgraph/nodes/twitch.rs)` の Helix 基盤を拡張する一連の作業として束ねた方が整理しやすい（旧 Phase ρ） |
| process spawn / kill / wait / window enum / move / resize / minimize / maximize / close / foreground / pseudo fullscreen | **Phase τ** | OS 依存（Windows 中心）で、非 Windows の degrade policy を doc レベルで決める必要あり（旧 Phase σ） |
| 汎用 Undo/Redo stack / 本物のマルチ選択 / Flowgraph subgraph (group) の第一級概念化 / ν-β で送った Svelte Flow handle drag edge E2E | **Phase υ** | 現行 flowgraphStore の履歴モデル（削除 1 段 snapshot）を全面改修する必要があり、単独フェーズが妥当（旧 Phase τ） |
| 物理 (spring / damper / integrator / gravity) / audio SE 再生 / 音量 envelope / ピッチ抽出 | **Phase ω** | ο の vec2/3 と signal util に直接乗る形で組めるため、ο 完了後が着手タイミング（旧 Phase υ） |
| Discord voice ingress / iFacialMocap 単独 / VTube Studio API / Global hotkey / MIDI / 独自 avatar renderer | **長期 backlog** | 単独で phase 立てるほど設計重量があり、かつ VAC の現役ユースケースに直結するかどうかも需要確認が必要 |

---

## 5. Architecture 判断

### 5.0 Phase ξ 先行（順序ブロッカー）

ο-1..ο-5 のノードは **Phase ξ 着地後**に着手する。plain-float で先行着地させて ξ-3 で全数 retrofit する案を検討したが、以下の理由で却下:

- ο-1 時点で 42 ノード分の `PortSpec` / テスト / `node-catalog.md` 定義を書くことになり、ξ-3 で **二度同じ分量**を書き直す作業が発生する
- retrofit によって既存 flow の TOML 定義が「値は同じだが型シグネチャだけ変わる」という **semantics-silent breakage** を起こす。deprecation cycle が増える
- trig / arctrig / normalize_angle は Phase ξ の Angle 次元がなければ **型安全性のうま味が一切取れない**。後付けで「Angle 次元必須」に変えた瞬間に既存 flow が動かなくなる（= ξ-3 が breaking change 化する）
- 最初から `Quantity<Dimension>` 対応で書けば、各ノードで `Quantity::new(result, input.unit())` で unit pass-through / `Quantity::dimensionless(result)` で無次元化を **ノードローカルに** 書けば済む。retrofit より実装コストが低い

### 5.1 int 版 / float 版の分離を維持

既存 `flowgraph.math.int_add` / `.float_add` の二元体系は継続する。追加する `abs` / `min` / `max` / `clamp` / `sign` も **int 版と float 版を別 feature として登録**する。理由:

- 既存ノードと一貫性が取れる（ユーザが palette で「int 用の clamp」「float 用の clamp」を曖昧に探さなくて済む）。
- 引数型が混ざる `Int + Float` は既存 `flowgraph.convert.*` の明示変換で扱う前提が spec 2.1 に固定されており、今フェーズでそれを緩める理由がない。

`lerp` / `inverse_lerp` / `remap` / `smoothstep` / 三角関数系 / 指数対数系は float のみ（int で数学的に意味を成さない / 精度が不足するため）。

### 5.2 easing: 1 ノード + curve プロパティ

§3.2 で述べた通り **単一 `flowgraph.easing.apply` + `curve` enum プロパティ**方式を採用する。拒否した代案:

- **1 curve = 1 node**: 18 ノード増。palette UX が悪化する。後発の curve 追加（例: `back_in/out/inout`）のたびにノード追加 PR が必要。
- **関数合成用 curve 値**: `SocketType::Curve` を新設する案。型システム拡張になり、narrow scope 違反。

trade-off メモ: GUI 側で curve を property editor の dropdown として出せるよう、property `curve` に `choices: Vec<String>` メタ情報を `PropertySpec` に持たせる必要がある可能性 → ο-0 時点で [`src/flowgraph/node.rs`](../../src/flowgraph/node.rs) の `PropertySpec` を覗いて「既存 `choices` フィールド有無」を確認するタスクを ο-2 冒頭に置く。

### 5.3 `rate_limit` との役割分担

| ノード | 責務 | 型 |
|---|---|---|
| `flowgraph.util.rate_limit` (既存) | **exec の発火頻度を抑える gate**。トークンバケット方式 | Exec → Exec |
| `flowgraph.util.throttle` (ο-5 新規) | **値が変化した時に値を流すが、interval_ms 未満で来たら捨てる** | Value → Value |
| `flowgraph.util.debounce` (ο-5 新規) | **変化が落ち着いて（deadtime_ms 静止）から値を流す** | Value → Value |

混同を避けるため、phase doc / node description 両方で「rate_limit は exec bucket、throttle/debounce は value stream」と明示する。

### 5.4 時刻まわりの "純粋性"（Phase π 後の正本）

**`flowgraph.datetime.now` は Pure 実装**（`Timestamp::now()`）。**非決定論**（毎回異なる）だが、I/O なし。spec 2.1 の「副作用なし」定義に合致。snapshot が必要なら `flowgraph.util.prev_value` 等（ο-5 予定）と組合せ。旧 §5.4 で検討していた `flowgraph.time.now_*` を Effectful にする案は、**`datetime` 型と π-5 ノード群導入により不要**。

- **`datetime.parse` / `format` / `diff` / `add_duration` / `sub_duration` / `epoch_ms` / `from_epoch_ms`**: すべて **Pure**（§5.2 と同階層のノード分類）
- **`flowgraph.util.timer_interval`**: backlog §1 通り **Stateful + self-trigger**（ο-4 実装済み、[`../../src/flowgraph/nodes/timer_interval.rs`](../../src/flowgraph/nodes/timer_interval.rs)）

### 5.5 依存追加方針

- `noise` crate: §3.6 の Perlin 実装。`noise = "0.9"` を想定。`rand` との API 非互換が有名なので ο-5 冒頭で小さな smoke test commit を挟む。
- **日時**: アプリ / Flowgraph の正本は **`jiff`**（Phase π で `chrono` 直接依存を剥がし済み）。Twitch 等の中間層に **間接的に `chrono` が残る**場合があり得る（π-3 時点の `cargo tree` 参照）が、**新規 Flowgraph ノードは jiff 経由の `DateTime` 境界型**に統一する。
- `rand` crate: 既存依存を再利用（fixture / OAuth state 生成で使用中）。

## 6. Sub-phase Breakdown

各サブは 1 commit 1 トピック（Commit Granularity Rule）。**ο-1 以降はすべて Phase ξ (ξ-0 〜 ξ-6) 着地を前提**（§5.0）。

| sub | 内容 | 触るもの |
|---|---|---|
| ο-0 | docs: 本 phase doc + roadmap.md 再編 + backlog-nodes.md §1 の pointer 化（+ 後日 angle normalization / 双曲線 10 ノード追記 + Phase ξ 依存明記）| `docs/roadmap/phase-omicron-flowgraph-enhancement.md` ✅ / `docs/roadmap.md` ✅ / `docs/roadmap/backlog-nodes.md` ✅ |
| ο-1 ✅ | feat(flowgraph/math): §3.1 **42 ノード**追加 + unit test（Quantity-aware）| `src/flowgraph/nodes/math.rs` / `src/flowgraph/registry.rs` |
| ο-2 ✅ | feat(flowgraph/easing): §3.2 `apply` ノード + curve 関数群（19 curve）+ unit test 13 件 / `PropertySpec.choices` + GUI dropdown hook | `src/flowgraph/nodes/easing.rs` (new) / `src/flowgraph/nodes/mod.rs` / `src/flowgraph/registry.rs` / `src/flowgraph/node.rs`（`PropertySpec.choices` + `with_choices`）/ `gui/src/lib/types.ts` / `gui/src/lib/flowgraph/FlowgraphPropertyEditor.svelte` |
| ο-3 ✅ | feat(flowgraph/vec): §3.3 **20 ノード**追加（vec2 10 + vec3 10。ο-0 の「18」は `add/sub` 1 行表記による行数ミス、実数は 20）+ unit test 19 件 | `src/flowgraph/nodes/vec.rs` (new) / `src/flowgraph/nodes/mod.rs` / `src/flowgraph/registry.rs` |
| ο-4 | feat(flowgraph/util): §3.4 `timer_interval` + unit test（**time 4 種は Phase π の `datetime` 8 ノードに移管済み、本サブでは実装しない**） | `src/flowgraph/nodes/timer_interval.rs` (new) / `delay.rs` パターン流用 / `engine.rs`（lazy 初回の `sources` 例外）/ `registry.rs` |
| ο-5 | feat(flowgraph/util,random,noise): §3.5 signal util 5 種 + §3.6 random/noise 5 種 | `src/flowgraph/nodes/signal.rs` (new) / `src/flowgraph/nodes/random.rs` (new) / `registry.rs` / `Cargo.toml`（`noise` 追加）|
| ο-6 | feat(gui): §3.7 palette カテゴリ絞り込み + canvas drop-at-cursor + Ctrl+D duplicate + E2E 回帰 | `gui/src/lib/flowgraph/FlowgraphPalette.svelte` / `FlowgraphCanvas.svelte` / `gui/src/lib/tabs/FlowgraphTab.svelte` / `gui/src/lib/flowgraphStore.svelte.ts` / `gui/tests/e2e/flowgraph-canvas-basic.spec.ts` |
| ο-7 | docs: CHANGELOG + `docs/manual/node-catalog.md` 再生成 + `docs/roadmap.md` tick | `CHANGELOG.md` / `docs/manual/node-catalog.md`（`BLESS_NODE_CATALOG=1` で再生成）/ `docs/roadmap.md` |

### 6.0 ο-0 チェックリスト（本セッション成果物）

- [x] `docs/roadmap/phase-omicron-flowgraph-enhancement.md` 新設
- [x] `docs/roadmap.md` の Active Phases を Phase ο に差し替え、Backlog / Future に π/ρ/σ/τ/υ + 長期 backlog を起こす
- [x] `docs/roadmap/backlog-nodes.md` §1 に「ο-4 で昇格」Status 注記を追加

### 6.1 ο-1 チェックリスト

- [x] **前提**: Phase ξ-3 (engine retrofit to Quantity) 着地済みであること。`SocketValue::Float` が `Quantity<Dimension>` を保持できる状態を前提に書く
- [x] 1-入力系 / 3-入力系のマクロを `math.rs` に追加（既存 2-入力マクロを踏襲、Quantity-aware 版）
- [x] **42 ノード**の `NodeDescriptor` + `PureNode::compute` 実装（内訳: abs/min/max/clamp/sign 系 int+float 合計 10 + lerp/inverse_lerp/remap/smoothstep 4 + trig 3 + arctrig 3 + atan2 1 + sqrt/pow/exp/ln/log2/log10 6 + floor/ceil/round 3 + deg_to_rad/rad_to_deg 2 + sinh/cosh/tanh 3 + asinh/acosh/atanh 3 + normalize_angle × 4）。ο-0 docs で "37 ノード" と誤記していたが、§3.1 の表を素直に列挙すると 42 ノードになる（ο-0 時点のカウントミス、実装時に再確認）
- [x] 次元制約（実装方針）: `sin/cos/tan` は **Angle 次元または dimensionless 入力を許容**（pre-ξ の plain-float flows の互換のため dimensionless はそのまま rad として扱う）、`asin/acos/atan/atan2` は **出力に Angle (rad) を付ける**、`normalize_angle_deg_*` / `_rad_*` は **Angle または dimensionless を受ける**（dimensionless は target unit のまま扱う）、`sinh/cosh/tanh/asinh/acosh/atanh` / `pow/exp/ln/log2/log10` は **dimensionless 必須**、`sqrt` は **`Quantity::try_sqrt` 委譲で次元対応**（全 atom exponent が偶数なら `sqrt(m\u{00B2}) = m` 等を返す、奇数 exponent は integer-dimension 制約で error、絶対温度 K も error; ο-1.1 で dimensionless-only から B 案へ格上げ）、`abs/min/max/clamp/floor/ceil/round/sign_*/lerp(a,b)/remap(out)` は **unit pass-through / 次元整合**、`inverse_lerp` / `smoothstep` は **入力同次元 → 出力 dimensionless**
- [x] `registry.rs` に登録（`register_pure`）
- [x] `cargo test --lib` の node 単体テストで各ノード 1-2 ケース（dimension mismatch error パスも含む、30 tests in `flowgraph::nodes::math::tests` 全緑）
- [x] `BLESS_NODE_CATALOG=1 cargo test` で `docs/manual/node-catalog.md` を再生成し、ο-7 まで blessed diff を保持

### 6.2 ο-2 チェックリスト

- [x] `PropertySpec` に `choices: Option<Vec<String>>` を追加（serde `skip_serializing_if = "Option::is_none"` 付き / `with_choices` ビルダー）。GUI 側 (`gui/src/lib/types.ts` の `FlowgraphPropertySpec.choices?: string[]` + `FlowgraphPropertyEditor.svelte` で `cat === 'string' && choices` の場合 `<select>` 描画) の dropdown hook まで一緒に着地
- [x] curve 関数群（linear + quad / cubic / sine / expo / elastic / bounce × in/out/inOut = 計 19 関数）を `apply_curve(Curve, f64) -> f64` helper として実装（Robert Penner 由来の定式）
- [x] `flowgraph.easing.apply` ノード実装 + `Curve::from_name` による string-to-fn 対応表 + unknown curve は `NodeExecError::Generic` で即停止（エラーメッセージに候補一覧を含む）
- [x] `clamp_t: bool = true` プロパティ反映（true 時は `t.clamp(0.0, 1.0)`、false 時は素通し → elastic/bounce の overshoot 観察用途）
- [x] unit test 13 件（全 curve endpoint 不変、`*_inout` の (0.5, 0.5) 通過、quad_in / cubic_in の明示値、out = 1 − in(1 − t) ミラー、clamp_t 挙動、elastic overshoot、bounce_out 区分点、unknown curve error、default curve = linear、spec が choices を宣言しているか、enum 全 19 round trip）

### 6.3 ο-3 チェックリスト

- [x] JSON 配列（Number × 2 / × 3）↔ `[f64; 2/3]` の decode / encode helper（`decode_vec::<N>` / `encode_vec::<N>`、非配列 / 長さ不一致 / 非数値 / 非有限値はすべて `NodeExecError::Generic` で即停止 — §3.3 安全方針通り）
- [x] vec2 **10 ノード** + vec3 **10 ノード**（計 20 ノード）を `const N: usize` generic helper + 6 種類のマクロ（`vec_make_node!` / `vec_unpack_node!` / `vec_binop_node!` / `vec_scalar_out_binop_node!` / `vec_scale_node!` / `vec_unary_pure_node!` / `vec_lerp_node!`）で実装。ο-0 docs の「vec 18」表記は `add` / `sub` を 1 行で書いたため表上の行数と実ノード数が 9 vs 10 でずれていた（math の「37 → 42」修正と同種の行数ミス）。実際は `make / unpack / add / sub / scale / dot / length / normalize / lerp / distance` で 10 ノード × 2 次元 = **20 ノード**
- [x] `make` / `unpack` を含めた round-trip unit test（vec2 / vec3 両方 / decode-encode 直接テストも含む）
- [x] `normalize` で零ベクトル扱いのドキュメンテーション: `NodeSpec.description` に "Zero vector returns [0, 0]/[0, 0, 0] (not an error)." を明記、unit test (`vec2_normalize_zero_vector_stays_zero` / `vec3_normalize_zero_vector_stays_zero`) で仕様を固定
- [x] `cargo test --lib flowgraph::nodes::vec` 19 tests 全緑、全体 695 tests 全緑、`BLESS_NODE_CATALOG=1 cargo test` で `docs/manual/node-catalog.md` 再生成

### 6.4 ο-4 チェックリスト

- [x] `flowgraph.util.timer_interval`（[backlog-nodes.md §1](backlog-nodes.md) 仕様）: Stateful + `ctx.trigger` 自己 re-arm + stale id drop + `enabled` / `__pending_id__` の stale 扱い + lib test（`run_forever` 実時間・複数 tick 等、tokio mock なし）
- [x] timer_interval: 実時間ベースの unit test（`run_forever` / interval clamp / enabled off / stale id。厳密な jitter 上限の数値 assert はスコープ外）
- [x] **日時 4 種（旧 `flowgraph.time.*` 案）**: **Phase π-5** で `flowgraph.datetime.*` 8 ノード + `DateTime` socket として**代替完了**（本フェーズのスコープ外。§3.4 / [`phase-pi-datetime-system.md`](phase-pi-datetime-system.md) §4.9 参照）
- [x] `backlog-nodes.md` §1 から本 doc への pointer の最終整備（π 移管後の文言に合わせる）

### 6.5 ο-5 チェックリスト

- [ ] `Cargo.toml` に `noise = "0.9"`（以上）を追加し、`cargo build --release` が通ることを確認
- [ ] signal util 5 ノード: state schema / ctx.trigger 使用箇所は `debounce` / `throttle` のみ
- [ ] random 3 ノード: `thread_rng` 使用、seed 化は将来 TBD として doc に
- [ ] noise 2 ノード: `Perlin::new(seed)` キャッシュ戦略（同一 seed は `OnceLock`）
- [ ] signal util の最小 integration test（tokio 実時間）

### 6.6 ο-6 チェックリスト

- [ ] palette カテゴリ絞り込み: `Set<string> hiddenCategories` + localStorage 永続化 + グループヘッダの toggle
- [ ] canvas drop handler: `dragover.preventDefault` + `drop` で feature 名取り出し + `screenToFlowPosition` で flow 座標
- [ ] palette 側 `dragstart` で feature 名 / spec ref を `dataTransfer` に入れる
- [ ] `Ctrl+D` / `Cmd+D` duplicate: FlowgraphTab の keydown に追加 + preventDefault + toast で「複製しました」
- [ ] `gui/tests/e2e/flowgraph-canvas-basic.spec.ts` に 3 項目のアサーションを追加（drop で位置確認 / Ctrl+D で id 変化確認 / palette カテゴリ toggle で表示行数変化）

### 6.7 ο-7 チェックリスト

- [ ] `CHANGELOG.md` の Unreleased に `### ο: Flowgraph Enhancement I (ο-0 .. ο-7)` 節を追加。カテゴリ別にノード一覧とサブフェーズの実装要約を記載
- [ ] `BLESS_NODE_CATALOG=1 cargo test flowgraph::docs::docs_tests::node_catalog_md_up_to_date` で `docs/manual/node-catalog.md` を最終更新
- [ ] `docs/roadmap.md` の Phase ο を "Active" から Completed に移動、π/ρ/σ/τ/υ の順序を見直し
- [ ] `svelte-check` / `cargo test --lib` / `npx playwright test` が全緑

---

## 7. Risks

| risk | 対策 |
|---|---|
| 大量の新ノード（ο: 42+1+20+1+5+5+5 = 74 想定＋ **Phase π の datetime 8** は別フェーズ、§3.4 / roadmap 参照）で `manual/node-catalog.md` の `BLESS_NODE_CATALOG=1` 再生成を ο-1 〜 ο-5 / π-5 ごとに忘れると、CRLF / LF 問題や diff 巨大化で cargo test が fail する | ο-1 / ο-2 / ο-3 / ο-4 / ο-5 / **π-5** の各 commit 前に `BLESS_NODE_CATALOG=1 cargo test` を必ず回し、blessed diff を commit に含める運用を phase doc 上で固定。ο-7 でまとめる誘惑に負けない |
| `noise` crate の API が将来版で change breakage | `noise = "0.9"` で固定（minor 上げは許容 / major は opt-in）。`Perlin::new(seed)` 以外の機能は触らない。将来 `simplex` / `worley` 等に広げるなら独立 PR |
| easing の `elastic` / `bounce` は `t ∈ [0, 1]` の外で発散 → テストのオーバーシュート判定で吸収するのを忘れると flaky | `clamp_t = true` を既定にし、代表 curve の "0.0 → 0.0、1.0 → 1.0" 境界値だけ assert する保守的テストに |
| `flowgraph.util.timer_interval` は Phase δ の `DelayNode` パターンに依存するが、`ctx.trigger` の node_fq 取得が実は未整備 | **着地済み**: `DelayNode` と同一経路。lazy 初回 execute のため `sources` に timer_interval を含める engine 例外を追加（ο-4） |
| GUI DnD drop は Svelte Flow の `screenToFlowPosition` が初期 viewport animation 中に呼ばれると座標が狂う | `FlowgraphAutoFit` の settle 完了を待つ flag を `flowgraphStore` に追加、drop 時に `if (!settled) return;` でハードフェイル（ν-β-2 と同様「flake 根源を殺す」方針） |
| Twitch / OpenAI / TTS など外部依存の重いフェーズ（φ / χ）中に Phase ο が走ると、Cargo build が遅くなる（単純な node 追加でも workspace 全体の再コンパイル発生） | narrow scope で crate 追加を `noise` 1 件に抑える。`Cargo.lock` 確認を ο-5 のチェックリストに |
| random / noise の "Pure vs Stateful" 議論が後付けで `SocketType::Seed` みたいな拡張に化ける | 今フェーズでは **Pure + thread_rng + 毎回違う値** で割り切り、seed 管理は Phase ο+ の TBD にする。phase doc §3.6 末尾に明記 |

---

## 8. References

- [`../roadmap.md`](../roadmap.md) の "Phase ο" セクション
- [`phase-ksi-dimensional-quantity-system.md`](phase-ksi-dimensional-quantity-system.md) — **Phase ξ（順序上の前提フェーズ）**。Quantity / Dimension / Unit 型、angle 疑似次元、strict default + 明示 escape hatch の設計元
- [`phase-delta-spec.md`](phase-delta-spec.md) — Flowgraph 型システム / Pure・Stateful・Effectful 分類の元仕様
- [`backlog-nodes.md`](backlog-nodes.md) — `flowgraph.util.timer_interval` 仕様（ο-4 実装と同期済みリファレンス）
- [`phase-nu-gui-e2e-playwright.md`](phase-nu-gui-e2e-playwright.md) — E2E fixture / spec 運用の前例
- [`../manual/node-catalog.md`](../manual/node-catalog.md) — 既存ノードカタログ（自動生成、ο-7 で再生成）
- [`../../src/flowgraph/nodes/math.rs`](../../src/flowgraph/nodes/math.rs) — 既存 math 実装（macro パターンの元ネタ）
- [`../../src/flowgraph/nodes/delay.rs`](../../src/flowgraph/nodes/delay.rs) — `ctx.trigger` self-trigger の参考実装
- [`../../src/flowgraph/registry.rs`](../../src/flowgraph/registry.rs) — `default_registry` 登録一覧
