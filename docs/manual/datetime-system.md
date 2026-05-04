# DateTime System — 絶対時刻を型で扱う

Virtual Avatar Connect の Flowgraph は、Phase **π** で **第一級ソケット型 `datetime`** を備えています。内部表現は [`jiff`](https://docs.rs/jiff) の **`Timestamp`（UTC 絶対時刻、ナノ秒精度）**で、**壁時計のインスタント**だけを扱います。タイムゾーン付きの「壁面表示」は `flowgraph.datetime.format` の `timezone` プロパティや `Zoned` 系の**境界**で扱います。

**経過時間（Duration）**は新しい型を増やさず、Phase ξ ですでにある **`quantity`（時間次元、例: `s`, `ms`）**に統一します。設計の動機と詳細は [`../roadmap/phase-pi-datetime-system.md`](../roadmap/phase-pi-datetime-system.md) を参照。

---

## なぜ String / float のままにしないのか

- RFC3339 文字列と「タイムゾーン無しの `2026-04-24 12:34:56`」を混在させると、**同じ見た目でも別の瞬間**になり得る。
- epoch 秒を `float` だけで扱うと、**ms / μs の境界**で単位事故が起きやすい（Phase ξ の「型は砦」思想と同じ理由）。

`datetime` ポートと `flowgraph.datetime.*` ノード群で、parse / diff / 加減算を一貫して表現します。

---

## 基本: `DateTime` と `Quantity<time>`

| 型 | 意味 | 例 |
|---|---|---|
| `datetime` | 絶対時刻（UTC インスタント） | ノード間で配線 |
| `quantity`（時間次元） | 持続・差分 | `1.5 s`, `500 ms` |

- **`datetime` − `datetime` → `quantity`（秒 `s`）** は `flowgraph.datetime.diff` が担当。必要なら `flowgraph.unit.convert` で `ms` 等に変換。
- **`datetime` ± `quantity`（時間次元）** は `add_duration` / `sub_duration`。`Quantity` は **同じ時間次元**（または互換表現）を要求。無次元 `quantity`（ξ-3 の `float` からの暗黙 coerce 経路）は、これらのノードでは **秒**として解釈されます。

---

## 設定: 既定タイムゾーン（naive 文字列用）

`conf.toml` の **`[flowgraph]`** テーブル（任意）:

```toml
[flowgraph]
default_timezone = "+09:00"   # 空 / 未指定 / "Z" / "UTC" は UTC
```

- **v0 は固定オフセット**（`+09:00`, `-05:30` など）のみ。**`Asia/Tokyo` など IANA 名は拒否**（`parse_offset_str` の仕様）。設定が不正な場合は起動時に warn して UTC にフォールバックする経路（`resolve_default_timezone_or_warn`）と、厳密検証用 API が分かれます。
- エンジン層の **String → `datetime` 暗黙 coerce**は **RFC3339 等の aware 文字列**を主対象にし、**naive 文字列**は厳格に扱います。naive を **`flowgraph.datetime.parse`** で読むとき、`default_timezone` プロパティ（ノード）が **空なら `+00:00` 相当**として解釈されます（ノード内で `parse_offset_str` 使用）。全体のデフォルト TZ を conf に寄せる運用と併用できます。

---

## ノード一覧（`flowgraph.datetime.*`）

| feature | 役割 |
|---|---|
| `flowgraph.datetime.now` | 現在時刻（非決定論; snapshot したい場合は下流で state 系と組合せ） |
| `flowgraph.datetime.parse` | 文字列 → `datetime`（aware / naive; `require_timezone` で strict） |
| `flowgraph.datetime.format` | `datetime` → 文字列（`rfc3339` / `iso8601_compact` / `unix_seconds` / `unix_millis` / `custom` + `timezone`） |
| `flowgraph.datetime.add_duration` | `datetime` + 時間 `quantity` |
| `flowgraph.datetime.sub_duration` | `datetime` − 時間 `quantity` |
| `flowgraph.datetime.diff` | `lhs` − `rhs` → 秒 `quantity`（ナノ秒精度を保持してから `f64` 秒化） |
| `flowgraph.datetime.epoch_ms` | `datetime` → `quantity`（単位 `ms`） |
| `flowgraph.datetime.from_epoch_ms` | `quantity`（時間または無次元=ms として）→ `datetime` |

旧 Phase ο-4 案の **`flowgraph.time.now_rfc3339` 等 4 種**は、上表の **now + format / now + epoch_ms / format / now + diff + unit.convert** の組合せに置き換えられます（π-5 で吸収）。

**Phase ο-4** では、残り **`flowgraph.util.timer_interval`（Stateful）** のみがスコープでした（別ノード; backlog 参照）。**ο-4 で実装済み**（`src/flowgraph/nodes/timer_interval.rs`）。

---

## よくあるパターン

1. **「今」を RFC3339 文字列にしたい**  
   `datetime.now` → `datetime.format`（`format = rfc3339`、UTC 表示なら `timezone` 空）

2. **「この時刻から何秒経ったか」**  
   過去の `datetime` を保持 → `datetime.now` と `diff` → 秒 `quantity` → 必要なら `flowgraph.unit.convert` で `ms`

3. **外部から来た「タイムゾーン無し」文字列**  
   `parse` の **`default_timezone`** をアプリの想定（例: `+09:00`）に合わせる。厳密に **必ずゾーン付きしか受け付けたくない**場合は `require_timezone = true`

---

## FAQ

- **Q. `DateTime` は serde / JSON では何になる？**  
  **RFC3339 文字列**（UTC は `Z` 終端）。`SocketValue` の wire も同方針です。

- **Q. なぜ IANA タイムゾーンはまだ使えない？**  
  v0 では **固定オフセットのみ**にして、DST や tzdb 起因の**暗黙の解釈差**を型システムの外に出さないため。将来の π+ で検討。

- **Q. chrono はまだ使われている？**  
  直接依存は削除済み。**間接的に** 一部 crate（例: Twitch クライアント）経由で残る可能性があります。Flowgraph / アプリの日時表現の正本は **jiff** です。

---

## 関連

- 単位次元（時間以外）: [Dimensional Quantity System](./dimensional-quantity-system.md)
- 全ノードのポート仕様: [Node Catalog](./node-catalog.md)（`./scripts/bless-node-catalog.ps1` で生成）
