# Phase ξ — Dimensional Quantity System (SI 準拠の単位次元システム)

> **Status**: ξ-0 docs 完了 / ξ-1 core types (Dimension / Unit / Quantity / parser) 着地 / ξ-2 `SocketType::Quantity` + `SocketValue::Quantity` + `flowgraph.unit.*` ノード 7 種着地。次は ξ-3 で `SocketValue::Float` → `Quantity` の既存コード migration。
> 起点: [`../roadmap.md`](../roadmap.md) の "Phase ξ" セクション。

---

## 0. Status

- Phase ν-β / ο-0 クローズ後の **次セッション Active 候補 No.1**。
- スコープは engine 側の型システム拡張 + 単位代数（Dimension / Unit / Quantity）+ 単位操作ノード群 + 既存 `SocketValue::Float` を `Quantity` に移行する refactor + log / format の unit-aware 化 + GUI ポートチップの unit バッジ表示。
- **Phase ο (Flowgraph Enhancement I) は本フェーズ完了後に着手**。ο の math / easing / vec / time / signal util ノードは最初から `Quantity<Dimension>` 対応で実装する（§5.0 ο-doc 参照）。

---

## 1. 動機

VAC は現時点では avatar / stream / twitch / dictionary が主なユースケースだが、作者意向として:

- 物理シミュレーションベースのシェーダー背景描画 / メガデモ的ビジュアル出力
- 光学・音響・古典力学の procedural アニメーション（Phase υ）
- Flowgraph を **汎用ノードベースプログラミング言語処理系**（Dr.USAGI ツール群の一般化）として発展

を射程に持つ。これらの文脈では「数値が何を表すか」を人間の暗黙知に頼ると、事故が **物理的・数学的に無意味な値の伝搬**として発現する:

| 典型的な事故 | 現状 | 単位システム導入後 |
|---|---|---|
| `sin` に deg 値を食わせる | 静かに `sin(45 degree as raw float)` ≈ 0.851 と誤った値が流れる | `sin` は Angle 次元 + rad 単位を要求 → 明示的変換なしでは engine error |
| 速度 `m/s` に加速度 `m/s²` を足す | エラーなく値だけ合算され、下流が誤った結論に | `m/s + m/s²` は次元不一致で即エラー、Log で `Dimension mismatch: L·T⁻¹ + L·T⁻²` と表示 |
| `km` と `m` を混ぜて加算 | 10,000 倍の誤差が静かに伝搬 | `1 km + 500 m` は両方 `L` 次元、prefix 違いは透過的に SI 正規化 → `1500 m` |
| 長さに時間を掛けて「速度」として扱う | 型で気づけない | `10 m × 2 s` → `Quantity { value: 20, dim: L·T, unit: "m·s" }` と自動組み立て、下流で速度扱いすれば次元不一致 |
| 周波数 Hz の値をそのまま周期扱い | 表示は数値のみで間違いに気づけない | Log が `60 Hz` / `1/60 s` を区別表示、Format ノードが unit 付きで整形 |

「型は生産性と安全性を守る砦」という作者の哲学 (D4) に沿い、**数値に単位を付け、単位は次元を持つ** 設計で Flowgraph を型システムとして格上げする。

### 1.1 runtime cost の懸念と Flowgraph 特性による償却

一般的な命令型言語では単位処理は実行時評価コストがかさむが、Flowgraph は:

- **Pure Node は副作用なしの DAG 評価** — 同一入力に対する計算は memoization 候補になる
- **遅延評価戦術** — 必要なサブグラフだけが実行される（枝刈り）
- **型制約は compile-at-load 時に解決可能** — グラフ load 時に `PortSpec` の Dimension を全連結に亙って伝搬 / 整合チェックすれば、runtime は数値演算 + unit metadata 受け渡しだけになる

結果、**unit-aware 計算の追加コストは実質「値の横に unit metadata を運ぶ」定数倍オーバーヘッド**に抑え込める。ボトルネックは metadata clone コストなので `Arc<UnitSpec>` / `Cow<'static, UnitSpec>` / unit intern table で逃げ切れる。作者の観察（原文: "Flowgraph は特に数値計算においては純粋関数型で遅延評価戦術を実現しているので評価の枝切りやメモ化による単位系計算数値計算全体の実行時最適化がほとんど自動的にかなり合理的に行われる"）はここを指している。

---

## 2. スコープ方針

**narrow-scoped, but foundational**。engine 型システムの拡張なので小さくはできないが、広げすぎない縛りを以下で固定する。

### 2.1 含むもの

- `Dimension` / `Unit` / `Quantity` 型の engine 実装（`src/flowgraph/quantity/` 新設）
- SI 7 基本次元 + Angle 疑似次元の 8 次元体系
- SI 接頭辞 20 種（Y, Z, E, P, T, G, M, k, h, da, d, c, m, μ, n, p, f, a, z, y）の compositional 扱い
- SI 基本単位（m, kg, s, A, K, mol, cd）+ 主要誘導単位（Hz, N, Pa, J, W, C, V, Ω, F, T, H, lx, Bq, Gy, Sv, kat）+ Angle 単位（rad, deg）+ 温度絶対 K / 温度差 ΔK(=ΔC)
- 無次元 `Dimensionless`（空次元）
- 単位操作ノード 6〜7 種（§4）
- 既存 `SocketValue::Float` → `SocketValue::Quantity` migration（dimensionless fallback で後方互換）
- `flowgraph.util.log` / `flowgraph.util.format` の unit-aware 化（unit 付きで整形表示）
- GUI ポートチップに unit バッジ表示（Float ポートが unit を持っていれば `"m/s²"` 等のラベル）

### 2.2 含まないもの（明示的に §5 で次フェーズへ送る）

- IO 系ノード（`flowgraph.channel.*` / `.ingress.*` / `.http.*` 将来の / `.twitch.*` / `.obs.*` 将来の / control API / JSON 組み立てノード）の **次元強制**
  - これらは **数値部分のみ見て pass-through**（入力 Quantity の value だけ使い、unit は透過的に運ぶか、明示的に `strip` された上で無次元数値として処理）
  - 理由: 外部 API / wire format 側が単位を持たないため、強制すると既存 flow と外部連携が全面破壊する
  - 将来個別に「ここは次元チェックしたい」案件が出たら **opt-in で逐次強化**（例: 将来の VMC pose 送信で位置が `m` 必須、角度が `rad` 必須 → ノード単位で dimension contract を持たせる）
- 温度の Celsius / Fahrenheit 単位表示（v0 は K / ΔK のみ、アフィン変換は ξ+）
- Angle 単位 `round` / `turn` / `grad` / `arcsec` / `arcmin`（v0 は rad / deg のみ、ξ+ で追加候補）
- ユーザ定義カスタム次元 / 単位（v0 は SI + Angle 固定、将来 `conf.toml` で宣言可能にする ξ+）
- GUI エディタ上での unit インライン入力 UX（property editor の手動テキスト入力で足りる v0、visual UX は Phase τ 合流候補）
- 複合単位の見た目（`kg·m/s²` vs `N` 表示切替）の UI 規則（v0 は "assign 時のユーザ指定名を優先、代数計算結果は分数形式" とシンプルに）

---

## 3. 型設計

### 3.1 `Dimension` — SI 7 基本次元 + Angle 疑似次元

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dimension {
    pub length: i8,                 // L (metre)
    pub mass: i8,                   // M (kilogram)
    pub time: i8,                   // T (second)
    pub current: i8,                // I (ampere)
    pub temperature: i8,            // Θ (kelvin, abs)
    pub amount: i8,                 // N (mole)
    pub luminous: i8,               // J (candela)
    pub angle: i8,                  // A (pseudo-dimension: plane angle)
}
```

指数は `i8`（`-128 .. +127`）で十分（実用範囲は `-4 .. +4`）。`Dimensionless` は全成分 0、`Length` は `length=1`、`Velocity` は `length=1, time=-1`、`Acceleration` は `length=1, time=-2`、`Force` は `length=1, mass=1, time=-2`。

**Angle を第 8 次元として扱う根拠**: SI では rad は dimensionless だが、Flowgraph 上では「`sin(50)` に deg 値が流れ込む事故」を型で防ぐため、疑似次元として独立に持つ。SI supplementary unit の思想に準拠。`round`（= 2π rad）のような他の angle 単位も同一次元として追加可能（v0 は rad, deg のみ）。

### 3.2 `Unit` — 構造化表現（Hybrid: 内部構造化 + 境界 parse/format）

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// 合成単位の分解: 基本単位への参照 × 指数。例: m/s² は {m: 1, s: -2}
    pub atoms: BTreeMap<BaseUnitId, i8>,
    /// SI 接頭辞。例: km = SIPrefix::Kilo + base m
    pub prefix: SIPrefix,
    /// 基本単位 × prefix では表せない非 SI 単位（deg = rad × π/180）の係数。ほとんど 1.0
    pub factor: f64,
    /// 表示名ヒント。ユーザが "N" と指定したら "N"、算術結果の "kg·m/s²" は derived で別
    pub display: Cow<'static, str>,
}

pub enum BaseUnitId {
    /// SI 基本単位
    Metre,
    Kilogram,
    Second,
    Ampere,
    Kelvin,
    Mole,
    Candela,
    /// Angle 疑似次元
    Radian,
    /// Temperature delta (次元としては Θ だが K と区別する ID)
    KelvinDelta,
    // 非 SI でも factor で収まるもの（factor 込み）
    Degree,    // factor = π / 180 (rad への換算係数)
}
```

- **内部は常に構造化** — `atoms + prefix + factor` で代数演算可能
- **比較は dimension 抽出して行う**: `unit_a.dimension() == unit_b.dimension()` で次元一致判定、unit 自体の名前は一致する必要なし（例: `N` vs `kg·m/s²` は同次元扱い）
- **wire 境界では parse/format** — TOML / JSON の文字列 `"m/s^2"` は parser で Unit 構造に変換、逆変換は canonical format で（§6.3）

### 3.3 SI 基本単位 + 主要誘導単位（v0 組み込み）

| SI 基本 | symbol | Dimension |
|---|---|---|
| metre | `m` | `length=1` |
| kilogram | `kg` | `mass=1` (prefix は `k`, base は `g` の組合せで表現) |
| second | `s` | `time=1` |
| ampere | `A` | `current=1` |
| kelvin | `K` | `temperature=1` (absolute) |
| mole | `mol` | `amount=1` |
| candela | `cd` | `luminous=1` |

| SI 誘導 | symbol | 組立 | Dimension |
|---|---|---|---|
| hertz | `Hz` | 1/s | `time=-1` |
| newton | `N` | kg·m/s² | `length=1, mass=1, time=-2` |
| pascal | `Pa` | N/m² | `length=-1, mass=1, time=-2` |
| joule | `J` | N·m | `length=2, mass=1, time=-2` |
| watt | `W` | J/s | `length=2, mass=1, time=-3` |
| coulomb | `C` | A·s | `current=1, time=1` |
| volt | `V` | W/A | `length=2, mass=1, time=-3, current=-1` |
| ohm | `Ω` | V/A | `length=2, mass=1, time=-3, current=-2` |
| farad | `F` | C/V | `length=-2, mass=-1, time=4, current=2` |
| tesla | `T` | kg/(A·s²) | `mass=1, time=-2, current=-1` |
| henry | `H` | V·s/A | `length=2, mass=1, time=-2, current=-2` |
| lux | `lx` | cd·sr/m² | `length=-2, luminous=1` |
| becquerel | `Bq` | 1/s | `time=-1`（Hz と同次元だが意味論的別名）|
| gray | `Gy` | J/kg | `length=2, time=-2` |
| sievert | `Sv` | J/kg | `length=2, time=-2`（Gy と同次元、別名）|
| katal | `kat` | mol/s | `time=-1, amount=1` |

Angle と Temperature（絶対 / 差分）は別建てで §3.5 / §3.6。

### 3.4 SI 接頭辞

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SIPrefix {
    Yotta,  // Y   10^24
    Zetta,  // Z   10^21
    Exa,    // E   10^18
    Peta,   // P   10^15
    Tera,   // T   10^12
    Giga,   // G   10^9
    Mega,   // M   10^6
    Kilo,   // k   10^3
    Hecto,  // h   10^2
    Deca,   // da  10^1
    None,   //     10^0
    Deci,   // d   10^-1
    Centi,  // c   10^-2
    Milli,  // m   10^-3
    Micro,  // μ   10^-6
    Nano,   // n   10^-9
    Pico,   // p   10^-12
    Femto,  // f   10^-15
    Atto,   // a   10^-18
    Zepto,  // z   10^-21
    Yocto,  // y   10^-24
}
```

- **Compositional**: `km = Kilo × m`、`MHz = Mega × Hz`、`μs = Micro × s`。`Unit` 構造体は接頭辞を分離保持し、数値比較時に SI base value へ正規化する
- **加減算での正規化規則**: 同次元で接頭辞が異なる場合、**どちらも SI base（prefix=None）に正規化してから計算、結果の表示 prefix は "入力のうちより大きい方"** を採る（`1 km + 500 m = 1.5 km`、`100 ms + 50 μs = 100.05 ms`）。作者判断（原文 "kmはkとmで組み立てて扱える状態にしておいた方が筋"）に沿う
- **kilogram の扱い**: SI 基本単位が `kg` なのは歴史的都合。内部的には `Unit { atoms: {Kilogram: 1}, prefix: None, .. }` が基本とし、`g` は `Unit { atoms: {Kilogram: 1}, prefix: Milli, .. }`、`mg` は `prefix: Micro` とする（`g` は `m` "相当" の位置）。**代数計算は `atoms` ベース**なので矛盾なし

### 3.5 Angle 単位（v0）

- `rad`: `Unit { atoms: {Radian: 1}, prefix: None, factor: 1.0, display: "rad" }`、Dimension `angle=1`
- `deg`: `Unit { atoms: {Degree: 1}, prefix: None, factor: π/180, display: "deg" }`、Dimension `angle=1`
  - 内部的には `Degree` も Angle 次元の基本単位扱い、ただし factor で rad への換算を持つ
  - 別案（採用候補）: `Degree` を持たず、`Unit { atoms: {Radian: 1}, prefix: None, factor: π/180, display: "deg" }` とする。**採用推し**: シンプル、atoms は radian 一本、factor で単位差を吸収
- 将来追加: `round` / `turn`（= 2π rad, factor = 2π）、`grad`（= π/200 rad）、`arcsec`、`arcmin`（ξ+ or user-defined）

### 3.6 Temperature: Absolute (K) vs Delta (ΔK)

作者判断（D2=B）により、**絶対温度と温度差を別 Unit として扱う**。

- `K`: `Unit { atoms: {Kelvin: 1}, .. }`、Dimension `temperature=1`、semantics: **absolute point**
- `ΔK` (= `ΔC`): `Unit { atoms: {KelvinDelta: 1}, .. }`、Dimension `temperature=1`、semantics: **delta/relative**

演算規則:

| 左辺 | 演算 | 右辺 | 結果 | 備考 |
|---|---|---|---|---|
| K | `+` | ΔK | K | 絶対 + 差分 = 新しい絶対 |
| K | `-` | K | ΔK | 絶対 - 絶対 = 差分（F# / Haskell-quantity 流） |
| K | `-` | ΔK | K | 絶対 - 差分 = 新しい絶対 |
| ΔK | `+` / `-` | ΔK | ΔK | 差分同士 |
| ΔK | `×` | Float | ΔK | スカラー倍は差分だけ可（絶対 × 2 は無意味） |
| K | `×` | Float | **error** | 絶対温度のスカラー倍は物理的に無意味（半分の絶対零度とは？）|
| K | `×` | K | **error** | 絶対温度 × 絶対温度は意味を持たない |

v0 では `°C` / `°F` は未サポート（アフィン変換の実装コスト削減）。ξ+ で `°C ↔ K` 変換ノードを追加する際は、`flowgraph.unit.convert_celsius_to_kelvin` 等の **独立ノード** として提供し、`flowgraph.unit.convert` の一般的変換経路には混ぜない（アフィン変換は乗算変換と semantics が違うため）。

### 3.7 Dimensionless

- `Dimension { 全成分 = 0 }` を特別扱い: **任意の単位への変換が自由**（無次元 → `m` / `s` / `rad` いずれも OK）
- 既存 `SocketValue::Float` で unit 未指定のものは dimensionless として扱う（後方互換）
- 割合・比率（例: `0.5` = 50%）、count（例: `42` 個）、`t ∈ [0, 1]` の easing 補間パラメータは全て dimensionless
- **SI 的には angle も本来 dimensionless だが**、Flowgraph では `angle=1` で別次元扱い（§3.1 Design 判断）

### 3.8 `Quantity` — 値 + 単位のペア

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub unit: Unit,
}

impl Quantity {
    pub fn dimensionless(value: f64) -> Self { /* unit = Unit::dimensionless() */ }
    pub fn of(value: f64, unit: Unit) -> Self { /* ... */ }
    pub fn dimension(&self) -> Dimension { self.unit.dimension() }
    pub fn as_si_base(&self) -> f64 { /* prefix * factor * value */ }
    pub fn convert_to(&self, target: &Unit) -> Result<Self, DimensionMismatch> { /* ... */ }
}
```

- `SocketValue::Float(f64)` → **`SocketValue::Quantity(Quantity)`** に migrate（§6.2）
- `Int` / `Bool` / `String` / `Json` / `List` / `Table` / `Exec` は変更なし
- `Quantity` のシリアライズ: `{ "value": 42.5, "unit": "m/s^2" }`（§6.4）

### 3.9 演算セマンティクス（`+`, `-`, `*`, `/`, `pow`）

| 演算 | 次元規則 | prefix規則 | エラー条件 |
|---|---|---|---|
| `a + b` | 両辺 Dimension 完全一致必須 | SI base に正規化、prefix は "大きい方" 継承 | 次元不一致 |
| `a - b` | 両辺 Dimension 完全一致必須 | 同上 | 次元不一致 |
| `a * b` | Dimension は成分ごとに加算、Unit atoms はマージ | prefix は和（`k × M = G`）、factor は積 | なし（ただし実質的に次元が大きくなりすぎて `i8` overflow は error）|
| `a / b` | Dimension は成分ごとに減算 | prefix は差 | b.value == 0 は既存の float_div と同じポリシー（NaN / 停止） |
| `pow(a, exp)` | exp は dimensionless 必須、a の Dimension は exp 倍 | exp は整数限定（`sqrt` / `cbrt` は別ノード、全 `i8` 成分が割り切れるか要チェック）| exp が非整数で Dimension が非 dimensionless、または `i8` 割り切れず |

**K と ΔK の特殊規則**は §3.6 の表を参照。

---

## 4. 新ノード

### 4.1 Core unit operations (`flowgraph.unit.*`)

| feature | 仕様 | 使用例 |
|---|---|---|
| `flowgraph.unit.assign` | 入力 `value: Float(dimensionless)` + property `unit: String`、出力 `result: Float(assigned unit)`。dimensionless でない入力は error | `60` (dimensionless) → assign "Hz" → `60 Hz` |
| `flowgraph.unit.convert` | 入力 `value: Float(some unit)` + property `target_unit: String`、出力 `result: Float(target unit)`。同次元間のみ。次元不一致は engine error | `1 km` → convert "m" → `1000 m`、`180 deg` → convert "rad" → `π rad` |
| `flowgraph.unit.strip` | 入力 `value: Float(any unit)`、出力 `result: Float(dimensionless)`。**明示 escape hatch**、unit を捨てて数値のみ伝搬 | `9.8 m/s²` → strip → `9.8` (dimensionless)、JSON 組み立て前などで使う |
| `flowgraph.unit.get_unit_string` | 入力 `value: Float`、出力 `name: String`。unit の canonical 表記（例: `"m/s^2"`） | Log 組み立て・デバッグ用 |
| `flowgraph.unit.get_dimension_string` | 入力 `value: Float`、出力 `dim: String`。Dimension の canonical 表記（例: `"L·T^-2"`） | Log 組み立て・デバッグ用 |
| `flowgraph.unit.same_dimension` | 入力 `a: Float, b: Float`、出力 `result: Bool` | 分岐 / 防御 |
| `flowgraph.unit.dimensionless` | 入力 `value: Float(any)`、出力 `result: Float(dimensionless)`。`strip` の別名でより「意図的に無次元化」を強調する sugar ノード。実装は `strip` に委譲（あるいは `strip` を廃止して本ノードに統一、判断は ξ-2 で）|

`assign` / `convert` の `unit` property は **String で単位名を受ける**（parser が走る）。GUI property editor は v0 では単なるテキスト入力、ξ+ で dropdown 候補化を検討。

### 4.2 将来候補（ξ+、本フェーズでは設計メモのみ）

- `flowgraph.unit.lift` — 自動 unit 組み立て（例: `value: 60, target_unit: "Hz"` で dimensionless → Hz assign の糖衣）
- `flowgraph.unit.decompose` — `1 N` を `{1, "kg", 1, "m", 1, "s", -2}` のように atoms 分解
- `flowgraph.unit.format` — Quantity を `{value, unit, precision, use_scientific}` で整形（`flowgraph.util.format` の dispatch で十分な可能性）
- `flowgraph.unit.temperature_c_to_k` / `.k_to_c` — アフィン変換の独立ノード（ξ+）

---

## 5. 非スコープ（明示的に次フェーズへ）

| 項目 | 送り先 / 方針 | 理由 |
|---|---|---|
| IO 系ノード（channel / ingress / http / twitch / obs / control API / JSON 組立）の次元強制 | **pass-through 方針を固定**。必要な箇所だけ将来 opt-in | wire format 側が単位を持たないため強制は既存 flow 全破壊。個別ノード案件として ρ / π / ο+ で逐次対応 |
| Celsius / Fahrenheit の単位表記 | **Phase ξ+**（独立ノードとして追加）| アフィン変換は乗算変換と意味論が違い、`convert` 一般経路に混ぜると設計が濁る |
| 非 rad/deg の Angle 単位（round, turn, grad, arcmin, arcsec）| **Phase ξ+ / user-defined** | v0 は rad + deg で VAC 現役ユースケースを網羅できる |
| ユーザ定義カスタム次元 / 単位（`conf.toml` で宣言） | **Phase ξ+** | エコシステムが成熟してから。当面は SI 固定で十分 |
| GUI で unit をインライン編集（ポート chip をクリックして dropdown） | **Phase τ 合流候補** | τ の Undo/Redo / multi-select と同じ画面操作レイヤ |
| unit 記号の代替表記（`kg·m·s^-2` vs `N`）の UI 切替 | **Phase ξ+** or **Phase τ** | 表示の冗長性 vs 可読性のトレードオフ、実使用してから決める |
| 有効数字 / 精度の propagation（`(5.0 ± 0.1) m` 系の誤差伝搬）| **長期 backlog** | 科学計算用途に寄るが VAC 現役文脈では over-engineering |
| 単位 x 複数の `value` をまとめた `Vector<Quantity>` 型 | **Phase ο の vec2/vec3 に吸収** | vec2/vec3 は成分ごとに Quantity を持つ（成分全員同一 Dimension） |

---

## 6. Architecture 判断

### 6.0 pure-functional + 遅延評価による runtime cost 償却

§1.1 参照。型制約は graph load 時に `PortSpec.dimension` で静的検証（DAG を走査して dimension 伝搬を事前解決）できる。runtime は:

- 値計算: `f64` の演算（既存と同コスト）
- unit metadata: `Arc<UnitSpec>` の clone / equality check（pointer 比較）
- 加減算時の SI 正規化: 1 乗算（prefix factor）

実測で懸念が出た場合の対策:

- unit intern table（`lazy_static!` / `OnceLock` で一次元ごとに unique `Arc<UnitSpec>` を確保）
- Dimension comparison: 64-bit 固定幅パックで `u64` 一発比較化（8 × `i8` = 8 bytes）
- memoization: 既存の Flowgraph engine が DAG evaluation cache を持っていれば unit 込みで cache key に

### 6.1 `uom` crate vs 自作

`uom` crate（SI 単位の Rust 実装、compile-time dimension checking）は成熟しているが:

| 観点 | `uom` 採用 | 自作 |
|---|---|---|
| compile-time vs runtime | compile-time dimension check（型パラメータ）| runtime dimension check（struct） |
| Flowgraph との相性 | 合わない（ノード同士の接続は runtime 検証で、graph load 時の型伝搬が必要）| 合う（runtime struct が graph engine の動的型システムと整合） |
| 接頭辞 | `si::length::kilometer` のように各単位を型として持つ | `SIPrefix` enum で compositional に扱える（D3 で作者が重視した設計） |
| Angle の扱い | SI 準拠 → dimensionless 扱いで Flowgraph の型安全性要求と矛盾 | Angle を第 8 次元に独立化できる |
| 温度 delta | `uom` の該当機能あり | 自前で明示的に別型化（3.6） |
| 依存削減 | crate 追加 | 外部依存ゼロ |

→ **推し: 自作**。compile-time と runtime の接続先が違いすぎるため `uom` は不向き。設計を完全にコントロールできる自作が合理的。ξ-1 冒頭で小さな `uom` prototype を書いて比較 benchmark を取ることも検討（10 分で見切りがつく）。

### 6.2 `SocketValue::Float` → `Quantity` の migration

**3-step 戦略**:

1. **ξ-1**: `Quantity` / `Dimension` / `Unit` を新規追加し、`SocketValue::Float(f64)` は変更せず **並走**。新規の `SocketValue::Quantity(Quantity)` バリアントを追加、`SocketType::Quantity` を型で導入
2. **ξ-3**: 既存の `SocketValue::Float` 参照箇所を `Quantity`（dimensionless default）に置換、`SocketType::Float` は `SocketType::Quantity` の alias として残す（deprecation）
3. **ξ-6**: 完全統一、`SocketType::Float` 削除（または dimensionless の alias として恒久残置、後方互換重視）

**既存 flow toml の後方互換**:

- `value = 42.5` リテラル → parser は `Quantity::dimensionless(42.5)` に解釈
- `value = { value = 42.5, unit = "m/s^2" }` → `Quantity::of(42.5, parse_unit("m/s^2")?)` に解釈
- `value = "42.5 m/s^2"` 文字列 sugar → パーサで `{value: 42.5, unit: "m/s^2"}` 等価

### 6.3 TOML wire format

検討中の 3 形式:

| 形式 | 例 | 長所 | 短所 |
|---|---|---|---|
| **A** plain + inline table | `42.5` or `{ value = 42.5, unit = "m/s^2" }` | 後方互換、既存数値はそのまま | 表現の揺れが大きい |
| **B** quoted string sugar | `"42.5 m/s^2"` | 単一形式、読みやすい | 数値が文字列化する違和感、parser エラー動線が複雑化 |
| **C** dedicated `[quantity]` table | `[[nodes.properties.threshold]] value = 42.5, unit = "m/s^2"` | 構造明示 | TOML 表現が冗長、既存との互換が壊れる |

→ **採用: A (plain + inline table)** を基本、**B (quoted string) を補助的に受理**（parser 互換）、**output canonical は A**。既存 plain リテラル flow は全部そのまま動く。

### 6.4 JSON wire format

外部 IO の JSON やりとりでは **pass-through**。Quantity の JSON 表現は 2 つ:

| 形式 | 例 | 用途 |
|---|---|---|
| **internal** | `{"value": 42.5, "unit": "m/s^2"}` | Flowgraph 内 / `flowgraph.util.log` 出力 / debug |
| **external** | `42.5`（value のみ）| IO 境界（`flowgraph.channel.post` / ingress / JSON 組立ノードから外部 REST API へ）|

- 既定: **`flowgraph.json.to_value` / `.stringify` は value のみ**（external 形式、unit は捨てる）
- `strip` を明示 prepend するのが推奨パターン、ただし自動的に strip しつつ warn ログも出す運用
- Quantity を internal 形式で JSON 化したい場合は新ノード `flowgraph.unit.to_json` を ξ-2 で提供（`{value, unit, dimension}` の 3 フィールド）

### 6.5 GUI ポート表示

- `FlowgraphNodeCard.svelte` のポート chip に **unit バッジ**を追加（unit が dimensionless 以外なら表示）
- 色分け: Dimension family ごとに色カテゴリ（Length=緑、Time=青、Mass=紫、Angle=黄、Temperature=赤、... とシンプルなマッピング）
- hover tooltip で Dimension canonical 表記（`L·T^-2` 等）
- property editor の unit フィールドは **text input**（v0）、dropdown 化は Phase τ の範疇

### 6.6 Strict default + explicit escape hatch（D4）

- 加減算: 次元不一致 → `NodeExecError::DimensionMismatch { lhs_dim, rhs_dim, op }`
- 乗除算: 型エラーにはならない（次元が自動組み立てされる）
- `flowgraph.unit.convert`: 同次元以外は `NodeExecError::DimensionMismatch { src_dim, target_dim }`
- escape hatch: `flowgraph.unit.strip` と `flowgraph.unit.dimensionless` で明示無次元化
- **GUI warnings**: 次元不一致エッジを load 時に検出して chip を赤表示する（ξ-5 GUI retrofit の範疇）

### 6.7 既存の `docs/manual/node-catalog.md` への影響

- 既存 `Float` ポートは全て dimensionless として自動表示（後方互換）
- 新 math / easing / vec / time / signal util ノード（Phase ο）は最初から Dimension を持つ
- node-catalog の自動生成ロジックに "Dimension 列" を追加（ξ-6 で `BLESS_NODE_CATALOG=1` 再生成）

### 6.8 ログ / フォーマット

- `flowgraph.util.log`: default output format は `"{value} {unit}"`（例: `"9.8 m/s^2"`）、unit が dimensionless なら数値のみ
- `flowgraph.util.format`: property で `include_unit: bool = true` を提供、`false` なら value のみ
- channel / outputs への送信: **unit 非付与**がデフォルト（外部連携は数値のみを期待）、`flowgraph.util.format` で明示 stringify してから流す

---

## 7. Sub-phase Breakdown

各サブは 1 commit 1 トピック（Commit Granularity Rule）。

| sub | 内容 | 触るもの |
|---|---|---|
| ξ-0 | docs: 本 phase doc + roadmap.md への Phase ξ 追加 + Phase ο doc の依存注記（ο 側は別 commit で実施済み）| `docs/roadmap/phase-ksi-dimensional-quantity-system.md` (new) / `docs/roadmap.md` |
| ξ-1 | feat(flowgraph/quantity): `Dimension` / `Unit` / `Quantity` + SI base/derived 定義 + prefix enum + parser + unit test | `src/flowgraph/quantity/mod.rs` (new) / `dimension.rs` (new) / `unit.rs` (new) / `quantity.rs` (new) / `parser.rs` (new) / `Cargo.toml` は変更なし（外部依存ゼロで自作） |
| ξ-2 | feat(flowgraph/nodes/unit): §4.1 単位操作ノード 6-7 種 + unit test | `src/flowgraph/nodes/unit.rs` (new) / `src/flowgraph/registry.rs` |
| ξ-3 | refactor(flowgraph): `SocketValue::Float` → `Quantity` migration + 既存ノード dimensionless fallback + 演算 trait に `Quantity + Quantity` 実装 + 既存 math/json/state テスト回帰 | `src/flowgraph/socket.rs` / `src/flowgraph/node.rs` / `src/flowgraph/nodes/math.rs` / `src/flowgraph/nodes/json_ops.rs` / `src/flowgraph/nodes/state.rs` / その他多数 |
| ξ-4 | feat(flowgraph/util): log / format の unit-aware 化 + channel stringify ルール確定 | `src/flowgraph/nodes/log.rs` / `format.rs` / `channel.rs` |
| ξ-5 | feat(gui): `FlowgraphNodeCard.svelte` ポート chip に unit バッジ + 色分け + hover tooltip + property editor の unit text input | `gui/src/lib/flowgraph/FlowgraphNodeCard.svelte` / `FlowgraphNodeProperty.svelte` / `flowgraphStore.svelte.ts` |
| ξ-6 | docs: CHANGELOG + `docs/manual/dimensional-quantity-system.md`（新設、ユーザ向け解説）+ `node-catalog.md` 再生成（Dimension 列追加）+ roadmap tick | `CHANGELOG.md` / `docs/manual/dimensional-quantity-system.md` (new) / `docs/manual/node-catalog.md` / `docs/roadmap.md` |

### 7.0 ξ-0 チェックリスト（本セッション成果物）

- [x] `docs/roadmap/phase-ksi-dimensional-quantity-system.md` 新設（本ファイル）
- [ ] `docs/roadmap.md` Active Phases に Phase ξ を追加（ο の上、先行ブロッカーとして）
- [x] Phase ο doc の依存注記（別 commit `d3f5de1` で着地済み）

### 7.1 ξ-1 チェックリスト

- [ ] `src/flowgraph/quantity/dimension.rs`: `Dimension` struct + `add_dim` / `sub_dim` / `mul_scalar` / `invert`
- [ ] `src/flowgraph/quantity/unit.rs`: `Unit` struct + `BaseUnitId` enum + `SIPrefix` enum + SI base/derived 定義テーブル
- [ ] `src/flowgraph/quantity/quantity.rs`: `Quantity` struct + `dimensionless` / `of` / `convert_to` / `as_si_base` / 演算 trait (`Add / Sub / Mul / Div`)
- [ ] `src/flowgraph/quantity/parser.rs`: 文字列 `"m/s^2"` / `"kg·m/s^2"` / `"μs"` 等のパーサ
- [ ] Temperature delta の型分離（`K` vs `ΔK` で `Kelvin` / `KelvinDelta` の別 `BaseUnitId`）
- [ ] unit test（少なくとも 30 本）: dimension 代数 / prefix 正規化 / SI base/derived round-trip / parser / convert_to 同次元・異次元 error / K + ΔK 演算
- [ ] `cargo build --release` が通る、`cargo test --lib flowgraph::quantity` 全緑

### 7.2 ξ-2 チェックリスト

- [x] `flowgraph.unit.assign` / `.convert` / `.strip` / `.get_unit_string` / `.get_dim_string` / `.same_dimension` / `.to_json`
      （`.dimensionless` は `.strip` と統合し別名化せず、`.to_json` が §6.4 internal form の明示出口として追加）
- [x] 各ノード unit test（`assign` 不正単位 / `convert` 次元不一致 / `convert` K⇄ΔK 拒否 / `convert` target_unit 必須 等 error 系含む）
- [x] `registry.rs` に登録 + `default_registry_contains_core_features` に 7 feature 追加
- [x] `BLESS_NODE_CATALOG=1 cargo test` で blessed diff（ξ-2 commit に同梱）
- [x] `SocketType::Quantity` + `SocketValue::Quantity(Quantity)` variant を socket.rs / node.rs / collection / state / table_ops に伝播（§6.2 が ξ-1 に置いていた "variant 追加" を ξ-2 冒頭で着地）
- [x] TOML wire format A + B 両対応（plain float → dimensionless / inline table `{value, unit}` / quoted string `"42.5 m/s^2"`）
- [x] JSON wire format internal `{value, unit}` encode + decode、`json_ops` / `table_ops` / `state` の pass-through 側は value-only（§6.4）
- [x] `Unit::to_si_base` / `from_si_base` を atom-canonical 係数込みで拡張（`Degree` → `Radian × π/180` 等、`convert deg rad` が正しく動くように）

### 7.3 ξ-3 チェックリスト

- [ ] `SocketValue::Quantity(Quantity)` バリアント追加（`Float` 並走、段階移行）
- [ ] `SocketType::Quantity { dim: Option<Dimension> }` 型追加（`dim: None` = 任意次元受け、`Some(d)` = 固定次元要求）
- [ ] 既存 `SocketValue::Float(f64)` の全使用箇所を audit、**内部的に dimensionless Quantity として扱う wrapper** を `SocketValueRepr` 変換に挟む
- [ ] `json_ops` は Quantity 受け取り時に value だけ使う実装（pass-through 方針固定）
- [ ] 既存 `flowgraph.math.*` の int_add 等は touched しない（Int は Quantity に乗らない）、float_add 等は Quantity 対応化
- [ ] `cargo test --lib` 既存回帰テスト全緑、`gui/tests/e2e/` も全緑

### 7.4 ξ-4 チェックリスト

- [ ] `flowgraph.util.log` default format: `"{value} {unit}"`
- [ ] `flowgraph.util.format` property `include_unit: bool = true`
- [ ] channel post 時の stringify は value のみ（明示 strip 推奨のドキュメント注記）
- [ ] unit test + integration test

### 7.5 ξ-5 チェックリスト

- [ ] `FlowgraphNodeCard.svelte`: ポート chip に unit バッジ（dimensionless 以外のみ）
- [ ] Dimension family 色分け（CSS variable / Svelte 側 static map）
- [ ] hover tooltip で Dimension canonical 表記
- [ ] property editor: unit フィールドの text input + parse error 表示
- [ ] `gui/tests/e2e/flowgraph-canvas-basic.spec.ts` に unit バッジ assertion 追加

### 7.6 ξ-6 チェックリスト

- [ ] `CHANGELOG.md` `### ξ: Dimensional Quantity System (ξ-0 .. ξ-6)` 節
- [ ] `docs/manual/dimensional-quantity-system.md` 新設（ユーザ向け解説: 動機、使い方、unit 指定方法、よくあるパターン、FAQ）
- [ ] `docs/manual/node-catalog.md` 再生成（Dimension 列追加、ο 依存）
- [ ] `docs/roadmap.md` tick

---

## 8. Risks

| risk | 対策 |
|---|---|
| `SocketValue::Float` の移行で既存テスト / flow が退行 | ξ-1/ξ-2 は並走、ξ-3 で慎重な段階移行。`cargo test` / `gui/tests/e2e/` を各 commit 前に全緑確認。dimensionless fallback を強固に |
| 単位パーサのエッジケース（`kg·m·s^-2` vs `kg m s^-2` vs `kg⋅m⋅s^(-2)`）の表記揺れ | parser は punctuation 寛容（`·`, `⋅`, `*`, space）、exponent は `^2` / `^-2` / `²` / `⁻²` を全て受理。canonical output は `·` + `^` 固定 |
| `μ`（U+03BC）vs `µ`（U+00B5）の Unicode 正規化 | parser で NFC 正規化をかけ、両方受け入れる。output は `μ` (U+03BC) |
| kilogram の特殊事情（SI 基本単位が k 付きで定義されている）| §3.4 に記した `kg` を 基本 / `g` を Milli 接頭辞で扱う規約を貫く。代数計算は `atoms` ベースなので矛盾なし |
| Angle を次元化したことで SI 互換性がやや崩れる | doc で明示、SI 表記（例: `rad` → 無次元）を期待する外部システムへの送信は `flowgraph.unit.strip` 経由にする運用で逃げる |
| 既存 flow の TOML で「`threshold = 42.5` が今までは単なる float だった」箇所が、ノード側で Dimension 要求を持つと読み込みエラーになる | 既存ノードは v0 では全て dimensionless を要求（`SocketType::Quantity { dim: None }` or `dim: Some(Dimensionless)` で合致）、ο 以降の新ノードだけ具体 Dimension を要求する。既存 flow は修正不要 |
| `uom` crate を後から採用したくなる可能性 | ξ-1 冒頭で prototype 比較 benchmark を小 commit で取る。コードは自作 / 外部 crate 切替可能な設計にはしない（シンプル優先） |
| GUI ポート chip が文字情報過密に | Dimension family 色分け + unit badge 短縮表示 + hover で全文、の 3 段階で濃淡をつける |
| Temperature delta vs absolute の UX 混乱 | `docs/manual/dimensional-quantity-system.md` の FAQ に典型例（`T1 - T2` の結果型、スカラー倍の可否）を書く |
| 多言語ロケールで `.` と `,` の扱い | 内部は `f64` / `{ .to_string() }` で `.` 固定、local format は GUI 層のみで表示時変換（v0 は `.` 固定、ξ+ でロケール対応） |
| 移行後 runtime cost 実測が仮説より悪化 | §6.0 の intern / packed dim / memoization を段階的に適用。最悪 Critical path だけ `f64` fast path を残す設計余地を持たせる |

---

## 9. References

- [`../roadmap.md`](../roadmap.md) の "Phase ξ" セクション
- [`phase-omicron-flowgraph-enhancement.md`](phase-omicron-flowgraph-enhancement.md) — ξ を前提とする次フェーズ（math / easing / vec / time / signal util ノード群が ξ Quantity に乗る）
- [`phase-delta-spec.md`](phase-delta-spec.md) — Flowgraph 型システム / Pure・Stateful・Effectful 分類の元仕様
- [`../../src/flowgraph/socket.rs`](../../src/flowgraph/socket.rs) — 既存 `SocketValue` / `SocketType` 定義（ξ-3 で拡張）
- [`../../src/flowgraph/node.rs`](../../src/flowgraph/node.rs) — 既存 `PortSpec` / `PropertySpec` 定義
- [`../../src/flowgraph/nodes/math.rs`](../../src/flowgraph/nodes/math.rs) — 既存 math 実装（ξ-3 で Quantity 対応）
- [`../manual/node-catalog.md`](../manual/node-catalog.md) — 既存ノードカタログ（ξ-6 で Dimension 列追加して再生成）
- [BIPM — SI Brochure 9th ed.](https://www.bipm.org/en/publications/si-brochure) — SI 基本単位・誘導単位・接頭辞の公式定義
- F# units of measure — 設計上の参考（コンパイル時解決が Flowgraph runtime と性質が違うことは認識済み、本フェーズは runtime struct ベース）
