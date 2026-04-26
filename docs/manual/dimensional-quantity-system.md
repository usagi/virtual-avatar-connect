# Dimensional Quantity System — Flowgraph で「単位」を第一級に扱う

Virtual Avatar Connect の Flowgraph は、Phase ξ で導入された **SI 準拠の単位次元システム**を備えています。数値には単位（`m`, `s`, `kg`, `m/s^2`, `rad` など）を貼り付けられ、単位は **次元**（Length · Mass · Time · Current · Temperature · Amount · Luminous · Angle の 8 成分）を持ちます。加算は次元一致を要求、乗除は次元を自動組み立て、表示は `"{value} {unit}"` 形式 — 要するに F# や Haskell の units-of-measure を Flowgraph の pure + 遅延評価エンジン上で走らせる、と思ってもらえれば大きく外れません。

このドキュメントは **使い方の正本**です。設計の動機や判断理由は [`roadmap/phase-ksi-dimensional-quantity-system.md`](../roadmap/phase-ksi-dimensional-quantity-system.md) にあります。

---

## なぜ単位を持つのか

数値計算のトラブル — センサー入力で `m/s` と `km/h` を混ぜた、`rad` と `deg` を取り違えた、時定数を `ms` と `s` で誤配線した、温度を `K` と `°C` で差したら +273 が出た — は **人類が脳内で暗黙に保持している「型」を落としてしまう**のが半分くらい原因、というのが VAC 作者の立場です。単位を値に貼り付けて型システムが見張れば、トラブルの発生点でエンジンが止まってくれます。

Flowgraph は pure-functional + lazy 評価なので、単位計算のランタイムコストは DAG 枝刈りと memoization で自然に償却されます。生産性と安全性を守るための「型の砦」としての単位システムを、現実的なコストで提供できるわけです。

---

## 基本：Quantity と Unit

- **Quantity** = 数値 + 単位。`42.5 m/s^2` のような「数と単位」の組。
- **Unit** = SI 基本単位（`m`, `kg`, `s`, `A`, `K`, `mol`, `cd`）+ Angle（`rad`, `deg`）+ `ΔK`（温度差）と、それらの積商・SI 接頭辞（`k`, `m`, `μ`, …）の組み立て。
- **Dimension** = 単位から算出される「何の量か」。同じ次元なら単位換算が可能（`1 km == 1000 m`）、違う次元同士の足し算はエラー。

### Flowgraph での型

| 型表記 | 説明 |
|---|---|
| `float`  | 無次元の `f64`。既存フローは全てこれ。 |
| `quantity` | 単位付き数値。Phase ξ 以降の新しい第一級型。 |

`float` と `quantity` は **暗黙的に相互変換**されます（後述 §「暗黙 coerce」）。既存のフローはそのまま動きます。

---

## Flowgraph エディタ（GUI）での見え方（Phase ξ-5）

- **Quantity ポート**は、専用の丸ハンドル色で `float` / `table` と区別されます。次元の先頭記号（`L`, `M`, `T`, …）に応じて **family ごとに色が変わります**（`node-catalog` のポート `default` が `{ value, unit }` で非無次元と復元できる場合のみ、その次元で色付け）。
- **単位バッジ**: 上記のように default から単位が分かるポートに限り、ラベル横に短縮表示されます。無次元のみのポートにはバッジは出ません。
- **ツールチップ**: ハンドルにカーソルを置くと、型に加えて次元 canonical（例 `L·T^-2`）や単位全文が分かる場合があります。
- **配線**: エンジンの [`SocketType::compatible_with`](../roadmap/phase-ksi-dimensional-quantity-system.md) と同じく、`float` ↔ `quantity`、`quantity` → `string` などがキャンバス上で接続可能です。**型が合わないエッジ**は赤の破線で表示されます（保存はユーザー責任: 実行時に engine が拒否する可能性あり）。
- **プロパティの単位文字列**: `flowgraph.unit.assign` の `unit`、`flowgraph.unit.convert` の `target_unit`、`flowgraph.util.format` の `unit_override` は、入力後に Control API の `GET /api/v1/control/flowgraph/parse-unit?text=...` で **サーバと同じパーサ**が検証します。無効ならエディタ直下にエラーが出ます（空の `unit` は無次元として valid）。

---

## 使える単位

### SI 基本 7 単位

| 記号 | 単位名 | 次元 |
|---|---|---|
| `m`   | メートル     | Length |
| `kg`  | キログラム   | Mass |
| `s`   | 秒           | Time |
| `A`   | アンペア     | Current |
| `K`   | ケルビン     | Temperature (Absolute) |
| `mol` | モル         | Amount |
| `cd`  | カンデラ     | Luminous |

### 角度（Angle 疑似次元）

| 記号 | 単位名 | 備考 |
|---|---|---|
| `rad` | ラジアン | SI canonical |
| `deg` | 度       | `1 deg = π/180 rad` |

角度は SI では無次元ですが、VAC では **「rad と deg を取り違えた」をエンジンで検知するため**に独立次元扱いしています。

### 温度差（ΔK）

| 記号 | 単位名 | 備考 |
|---|---|---|
| `dK` (または `ΔK`) | ケルビン差 | `K - K` の結果型 |

`K`（絶対温度）と `dK`（温度差）は **次元は同じでも意味が違う**ため、エンジンは別型として扱います。`K + K` は禁止（絶対温度の和は無意味）、`K - K = dK` / `K + dK = K` / `dK + dK = dK` / `K * 2 = K` は許容、といった具合に F# / Haskell-quantity 教科書的な意味論を踏襲します。

### よく使う誘導単位

| 記号 | 展開 | 次元 |
|---|---|---|
| `Hz`  | `1/s`          | 周波数 |
| `N`   | `kg·m/s^2`     | 力 |
| `Pa`  | `kg/(m·s^2)`   | 圧力 |
| `J`   | `kg·m^2/s^2`   | エネルギー |
| `W`   | `kg·m^2/s^3`   | 仕事率 |
| `C`   | `A·s`          | 電荷 |
| `V`   | `kg·m^2/(A·s^3)` | 電圧 |
| `ohm` (または `Ω`) | `kg·m^2/(A^2·s^3)` | 抵抗 |

### SI 接頭辞

| 記号 | 倍率 |
|---|---|
| `Y`, `Z`, `E`, `P`, `T`, `G`, `M`, `k`, `h`, `da` | 10^24 .. 10^1 |
| `d`, `c`, `m`, `u` (μ の ASCII 代替), `n`, `p`, `f`, `a`, `z`, `y` | 10^-1 .. 10^-24 |

`km`, `ms`, `us`, `MHz`, `GPa` のように単位記号の先頭に付けます。`μs` は Unicode のまま書けますし、ASCII だけで打ちたければ `us` でも同じ意味になります。

---

## 単位文字列の書き方（parser）

`flowgraph.unit.assign` や `flowgraph.unit.convert` の `unit` / `target_unit` プロパティに書く文字列は、以下の文法で書けます。

```
unit     = factor ( ('*' | '/') factor )*
factor   = (prefix)? base ( '^' integer )?
base     = 'm' | 'kg' | 's' | 'A' | 'K' | 'mol' | 'cd'
         | 'rad' | 'deg' | 'dK' | 'ΔK'
         | 'Hz' | 'N' | 'Pa' | 'J' | 'W' | 'C' | 'V' | 'ohm' | 'Ω'
         | ...
```

### 例

| 書き方 | 意味 |
|---|---|
| `m`          | 長さ（メートル） |
| `km`         | キロメートル（`m` に prefix `k`） |
| `m/s`        | 速度 |
| `m/s^2`      | 加速度 |
| `kg·m/s^2`   | 力（中点は `*` と同義、`kg*m/s^2` と書いても同じ）|
| `1/s`        | 頻度（= `Hz` と同次元）|
| `m^2`        | 面積 |
| `m^3`        | 体積 |
| `MPa`        | メガパスカル |
| `us`         | マイクロ秒（ASCII）|
| `μs`         | マイクロ秒（Unicode）|

スペースは許容されません（`"m / s"` は不可、`"m/s"` と書く）。文字列が空なら dimensionless です。

---

## ノードで単位を使う

### `flowgraph.unit.*` 操作ノード（7 種）

| feature | I/O | プロパティ | 用途 |
|---|---|---|---|
| `flowgraph.unit.assign`             | `float → quantity` | `unit` | dimensionless な数値に単位を貼り付ける |
| `flowgraph.unit.convert`            | `quantity → quantity` | `target_unit` | 同次元間の単位換算（例: `m/s → km/h`）|
| `flowgraph.unit.strip`              | `quantity → float` | — | 単位を捨てて生の数値に戻す（明示 escape hatch）|
| `flowgraph.unit.get_unit_string`    | `quantity → string` | — | 単位の canonical 文字列を取り出す |
| `flowgraph.unit.get_dim_string`     | `quantity → string` | — | 次元の canonical 文字列を取り出す（例: `"L·T^-2"`）|
| `flowgraph.unit.same_dimension`     | `(quantity, quantity) → bool` | — | 2 つの quantity が同次元か判定 |
| `flowgraph.unit.to_json`            | `quantity → json` | — | `{value, unit, dimension}` の internal 形式にダンプ |

### 計算ノード（Phase ξ-3 以降）

以下のノードは単位を解釈して計算します:

- `flowgraph.math.float_add` / `float_sub` / `float_mul` / `float_div`
  - 入出力ポートは `quantity`。`float` 入力は engine 側で dimensionless `quantity` に自動 wrap されます。
  - `add` / `sub` は両辺 **次元一致必須**。不一致は実行時エラー。
  - `mul` / `div` は **次元を組み立て**て返します（例: `m * s → m·s`、`m / s → m·s^-1`）。
  - `K + K`（絶対温度同士の和）、`0 除算` も実行時エラー。

### 書式ノード

- `flowgraph.util.format` (`quantity → string`): 単位付き文字列を整形。`include_unit: bool` で単位表示 ON/OFF、`precision: int` で小数点桁数、`unit_override: string` で表示単位を差し替え（同次元要）。

---

## 暗黙 coerce：既存フローとの後方互換

Phase ξ-3 で engine に「エッジの暗黙 coerce」を入れました。既存の `float` 型ポートと新しい `quantity` 型ポートが混ざっていても、以下のルールで自動的に橋渡しされます:

| 上流 → 下流 | 挙動 |
|---|---|
| `float → quantity`   | dimensionless `quantity` として wrap（単位なし、値のみ）|
| `quantity → float`   | **dimensionless に限り**値を取り出す。非 dimensionless だと実行時エラー、明示 `flowgraph.unit.strip` を要求 |
| `quantity → string`  | `Quantity` の `Display` 実装（`"{value} {unit}"` / dimensionless なら `"{value}"`）で文字列化 |
| `string → quantity`  | **非許容**（任意文字列から unit を安全に parse できないため）|

この結果、**既存の `float` を使っていたフローは一切改修なしで動き続けます**。新規に単位を入れたい箇所だけ `flowgraph.unit.assign` を挟めば OK です。

---

## flow TOML での quantity リテラル

`[[nodes]]` の `[nodes.inputs]` / `[nodes.properties]` で `quantity` 型を直接書きたい場合は、以下の 3 形式が使えます。

```toml
# A: 素の数値 → dimensionless quantity
[nodes.inputs]
value = 3.14

# B: inline table
[nodes.inputs]
force = { value = 9.81, unit = "m/s^2" }

# C: quoted string
[nodes.inputs]
gravity = "9.81 m/s^2"
```

ingress / channel.emit / json_ops のような外部 IO / JSON 系ノードは、互換性維持のため **`quantity` を受け取っても数値部分のみ**を通過させます（unit は捨てる）。unit を保存したい場合は `flowgraph.unit.to_json` で明示的に `{value, unit, dimension}` 形に変換してください。

---

## よくあるパターン

### センサー生値に単位を貼る

```
ingress.* ─ (float) ─▶ flowgraph.unit.assign ─ (quantity) ─▶ (次段の計算ノード)
                         unit = "m/s"
```

### deg と rad の取り違えを防ぐ

```
flowgraph.unit.assign (unit="deg") ─▶ flowgraph.unit.convert (target_unit="rad") ─▶ (三角関数)
```

`convert` は同次元間のみ。`unit="m"` の quantity を `target_unit="rad"` に渡せば実行時エラーになり、配線ミスがその場で止まります。

### Log に単位付きで流す

```
(quantity) ─▶ flowgraph.util.log  ← value ポートは string 型、engine が "{value} {unit}" に自動 stringify
```

精度や単位表示の ON/OFF を明示制御したい場合は `flowgraph.util.format` を挟みます。

```
(quantity) ─▶ flowgraph.util.format (include_unit=false, precision=2) ─▶ flowgraph.util.log
```

### Channel に単位なしで流す（字幕など）

```
(quantity) ─▶ flowgraph.util.format (include_unit=false) ─▶ (string) ─▶ flowgraph.channel.emit
```

または、engine の `Quantity → String` 暗黙 coerce に任せて単位付きで流す場合は `format` を省略して直接 `channel.emit` に繋いで OK（`"9.81 m·s^-2"` 形式で出力）。

### K と ΔK を混ぜる

```
(quantity K) ─▶ flowgraph.math.float_sub ─▶ (quantity ΔK)  ← K - K = ΔK
(quantity K) ─▶ flowgraph.math.float_add ← (quantity K)    ← 実行時エラー（K+K 禁止）
(quantity K) ─▶ flowgraph.math.float_add ← (quantity ΔK)   ← 許容、K を返す
```

---

## FAQ

**Q. 既存のフロー（すべて float）を書き換えないとダメ？**
いいえ。Phase ξ-3 の暗黙 coerce で `float` は dimensionless `quantity` として自動扱いされるため、一切書き換え不要です。新機能を使いたい箇所だけ `flowgraph.unit.assign` で単位を付けてください。

**Q. Celsius（°C）や Fahrenheit（°F）は？**
Phase ξ では未対応（`K` と `ΔK` のみ）。摂氏はアフィン変換（`°C = K - 273.15`）が必要で、単純な掛け算倍率では表現できないため意図的に送りました。Phase ξ+ または ο 以降で扱うかは未定です。

**Q. `round`（1 round = 360 deg = 2π rad）も欲しい。**
人類あるあるですね。角度次元は第一級なので `round` のような別 atom を追加する余地はあります。ただし Phase ξ では `rad` / `deg` のみ。`round` が必要になったら issue で相談してください。

**Q. `flowgraph.math.sqrt` / `pow` の単位の扱いは？**
- **`sqrt` は次元対応**。`sqrt(9 m²) = 3 m`、`sqrt(25 m²/s²) = 5 m/s` は通ります（`Quantity::try_sqrt` 委譲、Phase ο-1.1 で dimensionless-only から格上げ）。ただし全 atom exponent が偶数であることが必要で、`sqrt(4 m)` のような「半整数次元 `L^(1/2)`」は `Dimension` が 8 成分の `i8` で構成されている関係上表現できず、engine error で止まります。これを本当にやりたいなら `flowgraph.unit.strip` で明示的に dimensionless に落としてから `sqrt` してください。絶対温度 K の sqrt も禁止（`try_sqrt` 内でガード）。
- **`pow` は dimensionless-only**。`pow(2, 2.7)` のような非整数指数は出力次元を `D^2.7` のような半端な形にしてしまい、整数 Dimension では表現不能です。指数付き次元操作（`m³` を `m^(1/3)` に戻す等）は稀なので、必要なら `unit.strip` → `pow` → `unit.assign` の三段構えで明示するのが現行の方針。
- 我々が観測する物理世界の次元はほぼ例外なく整数で閉じているため、この制約が実用上のボトルネックになる場面はまずありません（詳細な議論は phase doc を参照）。

**Q. 性能は？**
Flowgraph は pure + 遅延評価なので、同じ入力に対する単位計算は DAG のメモ化で一度しか走りません。ホット経路で `quantity` の四則演算が毎フレーム叩かれるケースでも、入力が変わらなければ再評価されないので、F# 言語ほどのコンパイル時除去ではないものの実用上問題にはなりません（詳細は phase doc §1.1）。

**Q. 自分で unit を定義したい。**
Phase ξ では組み込みの SI 基本 + Angle + 主要誘導単位のみ。ユーザ定義次元 / 単位の拡張は設計を詰め切れていないため未実装。意見は issue で。

**Q. dimensionless に単位を付けたいことはある？**
`unit = ""`（空文字）を `flowgraph.unit.assign` に渡せば dimensionless のまま通ります（実質 identity 操作）。`ratio` や `fraction` のような「無次元だが意味のある量」に型を付けたい用途は Phase ξ では未対応で、tag / 命名ノード側で表現するしかありません。

**Q. エンジンが「次元不一致」でエラーを吐いた。どうする？**
1. 配線を見直す。本当に同じ次元のはずの 2 量を足そうとしているか？
2. 同次元なら `flowgraph.unit.convert` で単位を揃える。
3. 本当に単位を無視したい（e.g. 既存の文字列化パイプライン）なら、明示的に `flowgraph.unit.strip` で dimensionless に落としてから繋ぐ。
4. `flowgraph.unit.get_dim_string` を両端に挟んで `util.log` に流すと、次元が文字列で観測できます。

---

## 関連リンク

- [Node Catalog](./node-catalog.md) — 各 `flowgraph.unit.*` / `flowgraph.util.format` ノードの完全な port / property 一覧
- [Phase ξ 仕様書](../roadmap/phase-ksi-dimensional-quantity-system.md) — 設計判断の一次情報（Angle 疑似次元 / 温度 delta 分離 / strict default の根拠）
- [roadmap.md](../roadmap.md) — Phase ξ 全体の進捗
