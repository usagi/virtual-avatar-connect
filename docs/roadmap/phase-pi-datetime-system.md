# Phase π — DateTime Type System (jiff 採用 + chrono 全面置換)

> **Status**: π-0 docs 着地 / π-1 jiff deps + smoke test (14 tests) / π-2 chrono→jiff 全面置換 / π-3 chrono 直接依存解除 / π-4 Flowgraph 型基盤（`DateTime` socket + `FlowgraphInstanceConfig` + naive parse）/ **π-5** `src/flowgraph/nodes/datetime.rs` に 8 ノード + `get_required_datetime` + unit test 30 本、lib **800** tests green / **π-6** CHANGELOG + `docs/manual/datetime-system.md` + `manual/index.md` + omicron phase doc 更新 + roadmap tick。complete。
> 起点: [`../roadmap.md`](../roadmap.md) の **Completed Phases** 内「Phase π」節。実装順のメモ: Phase ο-3 完了 → Phase π → Phase ο-4（いずれも完了済み）。

---

## 0. Status

- Phase ο-3 (vec2/vec3 実装済み) の次フェーズ。**Phase ο-4 (time nodes) の前提**。
- スコープは (A) `chrono` crate の全面 `jiff` 置換 + `chrono` 依存解除、(B) Flowgraph に `SocketType::DateTime` / `SocketValue::DateTime` を第一級型として追加、(C) 設定デフォルト TZ 付きの naive datetime パース、(D) DateTime 演算ノード群、の 4 点。
- `Quantity<s>` (Phase ξ で導入済の時間次元数値) は **そのまま Duration として兼務**。DateTime 側は絶対時刻のみを表現し、Duration は既存 `Quantity<time>` に統合する（新しい型を追加しない）。

---

## 1. 動機

### 1.1 Flowgraph 観点

Phase ο-4 で予定されている `flowgraph.util.timer_interval` や `flowgraph.time.*` (now_rfc3339 / now_epoch_ms / format / since_ms) を素朴に実装すると:

- `now_rfc3339` は String、`now_epoch_ms` は Float で返すので型がバラバラ
- `format` は String → String 変換になり、間違った形式を食わせても静かに誤動作
- `since_ms` は Float 秒演算になり、精度欠損や単位事故を型で防げない

Phase ξ で「単位は型、事故を防ぐ砦」を貫いたのと同じ哲学を **時刻** にも適用する:

| 典型的な事故 | 現状 (String/Float 運用) | DateTime 型導入後 |
|---|---|---|
| RFC3339 と naive "2026-04-24 12:34:56" を混同 | parse は沈黙して成功 or 失敗でプリミティブ error | parse ノードが TZ 判定 → naive は config default tz を明示適用、RFC3339 はそのまま |
| タイムゾーン違いを足し算 | 誰も気づかない | `DateTime + DateTime` は型エラー (DateTime は Instant、足し算不能) |
| 秒数字列の dt 差 | Float 計算で精度欠損、秒単位に丸める | `DateTime - DateTime → Quantity<time>` 厳密、`Quantity<ms>` 等への変換は既存 unit 系ノードで安全 |
| DST で日またぎ誤算 | 時差を固定値 +9h で加算 | UTC absolute (`jiff::Timestamp`) を内部表現にして civil/zoned 変換は明示 API |

### 1.2 ライブラリ選定: なぜ `jiff` か

| 観点 | jiff | time | chrono (現行) |
|---|---|---|---|
| 設計思想 | TC39 Temporal 準拠、datetime の罠を型で潰す | chrono 軽量代替 | 歴史的経緯 API |
| Civil / Zoned / Timestamp | 型レベルで分離 | PrimitiveDateTime / OffsetDateTime | NaiveDateTime / DateTime`<Tz>` (混同しやすい) |
| Duration | `Span` (暦幅) / `SignedDuration` (絶対秒) 分離 | `Duration` 単型 | `chrono::Duration` (罠あり) |
| IANA tzdb | 標準搭載 (optional disable 可) | 非搭載 | `chrono-tz` で別 crate |
| FixedOffset 運用 | `jiff::tz::Offset::from_seconds` で即作成 | `UtcOffset::from_hms` | `FixedOffset::east_opt` |
| serde | `serde` feature で RFC3339 | `serde-human-readable` feature | `serde` feature (v0 から同梱) |
| 作者 / 実績 | BurntSushi (ripgrep / regex / walkdir) | time-rs project | chronotope |
| 型砦との親和性 | **★★★** | ★★ | ★ |

**結論**: Phase ξ の「単位は型」哲学に最も整合する `jiff` を採用する。既存 `chrono` は完全移行で剥がす（共存 debt を残さない）。

### 1.3 chrono 解除による副次効果

- `chrono` v0.4 系は歴史的に `time` v0.1 との API 二重化、Windows TZ API 周りのセキュリティ advisory 経由、`serde` 実装の `0.4 → 0.5` 破壊的互換性議論など、保守負担の源泉だった。
- `jiff` は依存が少なく、MSRV / serde / 全 OS tz 周りを作者がトータル設計している。長期保守性が上がる。
- ビルド時間: `chrono` は serde feature 込みで重いが、`jiff` の方が軽量 (具体値は π-3 で `cargo bloat` 計測する)。

---

## 2. スコープ方針

**narrow-scoped, but touches 21 files**。以下の 4 軸に切る:

### 2.1 含むもの

- (A) `Cargo.toml` 依存切り替え: `chrono` 削除 + `jiff = { version = "...", features = ["serde"] }` 追加 (最新版を π-1 で確定)
- (B) 既存 `chrono` 使用 21 箇所 (§5 に一覧) の `jiff` 相当への **全面置換**。既存の wire format / ログ出力 / ファイル名は **bit-for-bit 互換を死守** (serde JSON ファイルの round-trip test で担保)
- (C) Flowgraph engine の DateTime 第一級化:
  - `SocketType::DateTime` / `SocketValue::DateTime` (`jiff::Timestamp` wrap)
  - `FlowgraphInstanceConfig.default_timezone: Option<String>` (未指定時 UTC)
  - engine 側の `String ↔ DateTime` 暗黙 coerce (ξ-3 の Float ↔ Quantity と同じ形)
  - naive datetime 文字列は config default tz で解釈
- (D) DateTime ノード群 (§4):
  - `flowgraph.datetime.now` / `.parse` / `.format` / `.add_duration` / `.sub_duration` / `.diff` / `.epoch_ms` / `.from_epoch_ms` (計 8 ノード)
  - Phase ο-4 の `flowgraph.time.*` 4 種は本フェーズで型安全 DateTime ベースに格上げして吸収 → ο-4 は `flowgraph.util.timer_interval` 単独になる (ο 側 phase doc は π-6 で更新)

### 2.2 含まないもの (明示的に ξ+ / π+ 扱い)

- **IANA timezone 対応** (例: `"Asia/Tokyo"`): v0 は FixedOffset (`+09:00` 形式) のみ。IANA tzdb 連携は需要確認後に π+
- **DST (Daylight Saving Time) 計算**: FixedOffset 運用では DST は発生しない前提。IANA tz 導入時に合流
- **暦幅 Duration (`Span`)**: 「2 ヶ月後」「1 営業日」等の calendar-aware 幅計算は v0 では扱わない。Quantity`<time>` (絶対秒) で表現可能なもののみ
- **GUI での DateTime エディタ**: property editor は文字列入力で足りる。visual datetime picker は Phase υ 合流候補
- **マイクロ秒 / ナノ秒精度の厳密保証**: `jiff::Timestamp` は nanosecond 精度まで保持するが、ほとんどの wire format (RFC3339 subseconds) は millisecond 精度で丸める運用を既定とする (`Timestamp::strftime` で "%Y-%m-%dT%H:%M:%S%.3f%:z" 等を指定)

---

## 3. 型設計

### 3.1 内部表現

```rust
// crates/vac-core/src/datetime.rs（root の src/datetime/mod.rs は互換再エクスポート）

use jiff::Timestamp;

/// Flowgraph / engine 全体で使う絶対時刻。
/// 内部は UTC absolute (nanosecond 精度)。TZ 情報はフォーマット / 表示時のみ考慮。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DateTime(pub Timestamp);
```

- **UTC absolute 一本**: `jiff::Timestamp` は `i64` 秒 + `i32` ナノ秒の UTC 内部表現。TZ 情報は保持しない (`Zoned` とは別型)
- **比較・演算は Timestamp に委譲**: `PartialOrd` / `Ord` は Timestamp から derive、`Duration` 加減算は §3.3 参照
- **serde**: `Timestamp` の `serde` feature で RFC3339 (UTC `Z` 終端、nanosecond 精度) にシリアライズ。wire format 文字列で chrono::DateTime`<Utc>` と互換

### 3.2 Duration は `Quantity<time>` で兼務

```rust
// Duration は Phase ξ で既に Quantity<Dimension { time: 1 }> として導入済み。
// 本フェーズでは追加型を作らず、Quantity ↔ jiff::SignedDuration 変換関数だけ用意する。

pub fn quantity_to_signed_duration(q: &Quantity) -> Result<SignedDuration, DateTimeError>;
pub fn signed_duration_to_quantity_seconds(d: SignedDuration) -> Quantity;
```

- 変換は `Quantity<time>` の値を SI base 秒に正規化 → `SignedDuration::try_from_secs_f64`
- 次元不一致 (`s` 以外) は error
- 精度: `SignedDuration` は `i64` 秒 + `i32` ns の固定小数精度、`Quantity<time>` は `f64` 秒。float ↔ fixed 往復で端数が発生する場合があるため、`.diff` ノード結果はナノ秒精度で float 化する (`f64::from(ns as i128) / 1e9`)

### 3.3 DateTime 演算ルール

| 演算 | 入力 | 出力 | 備考 |
|---|---|---|---|
| `DateTime + Quantity<time>` | 絶対時刻 + Duration | 絶対時刻 | `jiff::Timestamp::checked_add(SignedDuration)` |
| `DateTime - Quantity<time>` | 絶対時刻 - Duration | 絶対時刻 | 同上 checked_sub |
| `DateTime - DateTime` | 絶対時刻 - 絶対時刻 | Quantity<time> | `jiff::Timestamp::duration_since` → secs + ns → f64 秒 |
| `DateTime + DateTime` | 未定義 | error | 型エラー、ノード側でも提供しない |
| `DateTime × Float` | 未定義 | error | 同上 |

### 3.4 SocketType / SocketValue

```rust
pub enum SocketType {
    // 既存: Float, Int, Bool, String, Json, Table, Quantity, ...
    DateTime,
}

pub enum SocketValue {
    // 既存 ...
    DateTime(DateTime),
}
```

- **wire format (JSON)**: `{"type": "datetime", "value": "2026-04-24T12:34:56.123456789Z"}` または直接 RFC3339 文字列で外部 IO 互換
- **TOML fixture**: `{ datetime = "2026-04-24T12:34:56+09:00" }` または `{ datetime_utc = "2026-04-24T03:34:56Z" }`
- **Engine coerce** (ξ-3 と同じ形):
  - `String → DateTime`: RFC3339 parse、TZ 省略時は config default tz 適用、失敗時 coerce error
  - `DateTime → String`: `Timestamp::to_string()` (RFC3339 UTC)
  - `DateTime → Quantity<time>` (epoch 秒): 明示ノードのみ (自動 coerce はしない、意味が広すぎる)

### 3.5 Config default timezone

```rust
// crates/vac-flowgraph/src/config.rs

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FlowgraphInstanceConfig {
    // 既存フィールド ...

    /// 数値または naive datetime 文字列を DateTime に coerce する際の既定 TZ。
    /// 未設定時は UTC (`Z` / `+00:00`) 扱い。書式は RFC3339 相当の "+HH:MM" または "Z"。
    pub default_timezone: Option<String>,
}
```

- **パース形式**: `"+09:00"` / `"-05:30"` / `"Z"` / `"+00:00"`。RFC3339 offset 形式のみ。IANA tz (`"Asia/Tokyo"`) は v0 では **error**
- **評価タイミング**: config load 時に独自パーサで検証、失敗時は warn ログ + UTC fallback (起動は続行)
  - **π-1 で判明**: `jiff::tz::Offset` は `FromStr` を実装しない。`jiff::fmt::temporal::DateTimeParser::parse_time_zone` は `"+09:00"` / `"-05:30"` を `TimeZone` として受理するが、**bare `"Z"` は拒否**し、IANA zone (`"Asia/Tokyo"`) は受理してしまう
  - よって config 層で以下の独自パーサを書く (π-4 で実装):
    1. 入力 `s` が `"Z"` / `"UTC"` / `""` → `Offset::UTC`
    2. `DateTimeParser::new().parse_time_zone(s)` で `TimeZone` を得る → `.to_fixed_offset()` で `Offset` を取り出す
    3. `to_fixed_offset()` が失敗 (IANA だった) → config error として reject
  - `tests/jiff_smoke.rs::datetime_parser_parse_time_zone` で上記 API 挙動を pin 止め
- **適用先**: naive datetime 文字列の parse 時のみ。既に TZ 情報を持つ RFC3339 文字列はそのまま尊重

### 3.6 naive datetime の取り扱い

parse 対象文字列の TZ 情報の有無を以下で判定:

| 入力文字列 | 判定 | 適用 TZ |
|---|---|---|
| `"2026-04-24T12:34:56Z"` | aware (Z) | UTC |
| `"2026-04-24T12:34:56+09:00"` | aware (offset) | +09:00 |
| `"2026-04-24T12:34:56"` | naive | config default tz (未設定なら UTC) |
| `"2026-04-24 12:34:56"` | naive (space separator) | 同上、受理 |
| `"2026-04-24"` | naive (date only) | 同上、時刻 00:00:00 補完 |
| `"2026/04/24 12:34:56"` | 非 ISO | error (v0 は ISO 8601 系のみ) |

- **実装**: `jiff::civil::DateTime::from_str` で naive パース試行 → 成功なら config default tz を適用して `Zoned` → `to_timestamp()`。失敗時は `Timestamp::from_str` で aware パース試行
- **strict モード**: `flowgraph.datetime.parse` の property `require_timezone: Bool = false` で naive を拒否できる (後方互換上、既定は false)

---

## 4. DateTime ノード群 (π-5)

計 **8 ノード**。全て Pure、engine 内完結、外部 IO なし。

### 4.1 `flowgraph.datetime.now`

- **入力**: なし
- **出力**: `datetime: DateTime` (現在の UTC absolute)
- **実装**: `Timestamp::now()` を wrap
- **性質**: 非決定性。ο-5 の `prev_value` / `sample_hold` と組み合わせて snapshot 用途

### 4.2 `flowgraph.datetime.parse`

- **入力**: `s: String`
- **プロパティ**: `require_timezone: Bool = false` (strict モード)
- **出力**: `datetime: DateTime`
- **挙動**: §3.6 のルールで naive/aware 判定 → config default tz fallback
- **エラー**: parse 失敗 / `require_timezone=true` で naive 入力 → `NodeExecError::Generic`

### 4.3 `flowgraph.datetime.format`

- **入力**: `datetime: DateTime`
- **プロパティ**:
  - `format: String`, choices = `["rfc3339", "iso8601_compact", "unix_seconds", "unix_millis", "custom"]` (既定 `"rfc3339"`)
  - `custom_format: String` (`format = "custom"` 時の strftime 相当)
  - `timezone: String = ""` (空なら UTC、`"+09:00"` 等指定時はその TZ に変換してから format)
- **出力**: `text: String`
- **実装**: `jiff::Zoned::strftime` 経由。`timezone` は `jiff::tz::Offset::from_str`

### 4.4 `flowgraph.datetime.add_duration`

- **入力**: `datetime: DateTime`, `duration: Quantity` (次元 `time=1`)
- **出力**: `result: DateTime`
- **エラー**: duration 次元不一致、加算オーバーフロー

### 4.5 `flowgraph.datetime.sub_duration`

- add_duration と対称

### 4.6 `flowgraph.datetime.diff`

- **入力**: `lhs: DateTime`, `rhs: DateTime`
- **出力**: `duration: Quantity` (単位: 秒、次元 `time=1`)
- **実装**: `lhs.0.duration_since(rhs.0)` → ナノ秒精度で f64 秒化

### 4.7 `flowgraph.datetime.epoch_ms`

- **入力**: `datetime: DateTime`
- **出力**: `millis: Quantity` (単位: ms、次元 `time=1`) ※ 呼び出し側で `unit.convert` で s / us / ns に変換可
- **実装**: `Timestamp::as_millisecond()`

### 4.8 `flowgraph.datetime.from_epoch_ms`

- **入力**: `millis: Quantity` (次元 `time=1`) または `Float` (dimensionless は ms として解釈、ξ-3 暗黙 coerce)
- **出力**: `datetime: DateTime`
- **実装**: `Timestamp::from_millisecond(i64)` (range out で error)

### 4.9 ο-4 側の削減

ο-4 当初案の `flowgraph.time.*` 4 種 (now_rfc3339 / now_epoch_ms / format / since_ms) は **本フェーズで吸収**:

| ο-4 案 | π-5 置換 |
|---|---|
| `flowgraph.time.now_rfc3339` | `flowgraph.datetime.now` + `flowgraph.datetime.format` (format=rfc3339) |
| `flowgraph.time.now_epoch_ms` | `flowgraph.datetime.now` + `flowgraph.datetime.epoch_ms` |
| `flowgraph.time.format` | `flowgraph.datetime.format` |
| `flowgraph.time.since_ms` | `flowgraph.datetime.now` + `flowgraph.datetime.diff` + ξ の `flowgraph.unit.convert`(s → ms) |

→ Phase ο-4 scope は `flowgraph.util.timer_interval` 単独に縮減。omicron doc は π-6 で更新。

---

## 5. chrono → jiff 移行対応表 (21 箇所)

| # | ファイル | 行 | chrono | jiff 置換 | 備考 |
|---|---|---:|---|---|---|
| 1 | `src/message.rs` | 1 | `use chrono::{DateTime, Utc}` | `use jiff::Timestamp` + 型別名 | 当時の互換確認対象。後続の構造整理で未使用ファイルとして削除済み |
| 2 | `src/state/channel_datum.rs` | 3 | `use chrono::{DateTime, Utc}` | `use jiff::Timestamp` | struct field `DateTime<Utc>` → `Timestamp` |
| 3 | `src/web_interface/ws.rs` | 6 | `use chrono::{DateTime, Utc}` | `use jiff::Timestamp` | |
| 4 | `src/web_interface/output.rs` | 92 | `retrieved_timestamp.parse::<chrono::DateTime<chrono::Utc>>().unwrap()` | `retrieved_timestamp.parse::<Timestamp>()?` | unwrap → ? (呼出元 Result 化必要) |
| 5 | `src/web_interface/control/ws/actor.rs` | 該当行 | `chrono::Utc::now().to_rfc3339()` | `Timestamp::now().to_string()` | RFC3339 互換、subsec 有無要確認 |
| 6 | `src/web_interface/control/ping/mod.rs` | 該当行 | 同上 | 同上 | |
| 7 | `src/web_interface/control/dto/mod.rs` | 該当行 | 同上 | 同上 | |
| 8 | `src/web_interface/control/restart/mod.rs` | 該当行 | `chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()` | `Timestamp::try_from(t).map(\|ts\| ts.to_string())` | `SystemTime` → `Timestamp` 変換は `try_from` |
| 9 | `src/web_interface/control/oauth_twitch/mod.rs` | 該当行 | `use chrono::{DateTime, Utc}` | `use jiff::Timestamp` | |
| 10 | `src/web_interface/control/profiles/util.rs` | （`backup_path`） | `chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()` | `Zoned::now().strftime("%Y%m%d-%H%M%S").to_string()` | Local → `Zoned::now()` は system tz 利用 |
| 11 | `src/web_interface/control/run_with/mod.rs` | 該当行 | 同上 | 同上 | |
| 12 | `src/web_interface/control/flowgraph/mod.rs` | （分割後） | 同上 | 同上 | |
| 13 | `src/flowgraph/nodes/dictionary.rs` | 66〜78 | `DateTime<Utc>` / `parse_from_rfc3339` / `Utc::now().to_rfc3339_opts(Secs, true)` | `Timestamp` / `s.parse::<Timestamp>()` / `Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()` | `to_rfc3339_opts(Secs, true)` の秒切捨て + Z 終端を strftime で再現 |
| 14 | `src/flowgraph/nodes/dictionary.rs` | 104, 636 | `chrono::Utc::now()` | `Timestamp::now()` | |
| 15 | `src/flowgraph/nodes/table_ops.rs` | 404 | `chrono::Utc::now().to_rfc3339_opts(Secs, true)` | `Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()` | |
| 16 | `src/flowgraph/fragment/mod.rs` | 555 | 同上 | 同上 | |
| 17 | `src/flowgraph/tts/driver.rs` | 146 | `chrono::Utc::now().to_rfc3339().replace([':', '-'], "")` | `Timestamp::now().to_string().replace([':', '-'], "")` | |
| 18 | `src/flowgraph/tts/drivers/voicepeak.rs` | 111 | `timestamp_nanos_opt() / timestamp_micros()` | `Timestamp::now().as_nanosecond()` / `as_microsecond()` | 返値は `i64` / `i128`、既存 fallback ロジック維持 |
| 19 | `src/flowgraph/nodes/screenshot.rs` | 85 | `chrono::Utc::now().to_rfc3339().replace([':', '-'], "")` | `Timestamp::now().to_string().replace([':', '-'], "")` | |
| 20 | `src/managed_app/mod.rs` | 40 | `use chrono::{DateTime, Utc}` | `use jiff::Timestamp` | |
| 21 | `src/runtime.rs` | 11 | `use chrono::Utc` | `use jiff::Timestamp` | |
| 22 | `src/bin/migrate_dict.rs` | 177 | `chrono::Utc::now().to_rfc3339_opts(Secs, true)` | `Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()` | |

**フォーマット互換性の担保**:

- `chrono::Utc::now().to_rfc3339()` は `"2026-04-24T03:34:56.123456789+00:00"` 形式 (nanosecond 精度、offset `+00:00`)
- `jiff::Timestamp::now().to_string()` は `"2026-04-24T03:34:56.123456789Z"` 形式 (末尾 `Z`)
- **差異**: offset 表示が `+00:00` vs `Z`。どちらも RFC3339 valid だが、既存ログ / 外部連携で固定形式を前提にしている箇所があれば strftime で揃える
- **π-2 の対応**: 各置換箇所に対して **「置換前後で比較するキーが実行時に変わらないか」** を個別テストで担保。特に:
  - `message.rs` / `channel_datum.rs` の serde 出力 (snapshot test)
  - `dictionary.rs` の TTL 比較 (文字列比較か Timestamp 比較かで semantics が変わりうる → Timestamp 比較に統一推奨)
  - `output.rs` の parse は `Timestamp::from_str` で必ず RFC3339 を要求 (既存は chrono が許容していた幅広い形式を想定している可能性 → 要確認)

---

## 6. サブフェーズ分割

### π-0 — docs (本ドキュメント)

- 本ファイル (`phase-pi-datetime-system.md`) 新設
- `roadmap.md`:
  - "Active Phases" に Phase π セクション新設 (Phase ο と並列)
  - 旧 Phase π (OSC/VMC) を Phase ρ にリネーム、以下 ρ→σ→τ→υ→ω に 1 文字ずつ繰り下げ
  - Unscheduled Flowgraph Nodes の "Phase ο-4 で timer_interval 昇格" を π-5 側での time nodes 吸収と併せて注記
- `backlog-nodes.md`: 該当箇所があれば Phase π 依存を追記
- この docs commit (π-0) の時点では Phase ο-4 omicron doc は触らない (π-5 側で time nodes を実装する時に更新)

### π-1 — feat(deps): jiff 追加

- `cargo add jiff --features serde` で最新版を追加 (本 doc 書き時点では未確定、π-1 commit で具体版を固定)
- `Cargo.lock` 更新
- 既存 `chrono` は残したまま。この時点では jiff は unused で warn が出るため、`#[allow(unused)]` 付きの sanity test を `tests/jiff_smoke.rs` に 1 本置く (`Timestamp::now()` と `Timestamp::from_str("2026-04-24T00:00:00Z").unwrap()` のみ)
- **検証**: `cargo build --all-features` + `cargo test --lib` green

### π-2 — refactor: chrono → jiff 全面置換

- §5 の 22 箇所を 1 ファイルずつ置換。各ファイル置換後に `cargo check` を通す
- **順序の原則**: leaf (呼ばれるだけのファイル) から書き換える。型定義を持つ `message.rs` / `channel_datum.rs` / `managed_app/mod.rs` は最後。理由: 型を先に変えると下流の `cargo check` が長期間赤くなる
- **snapshot test 追加**: serde 経由で `DateTime<Utc>` を JSON に書き出していた構造体 (特に `channel_datum::ChannelDatum`) について **既存 chrono 出力 / 新 jiff 出力 の JSON バイト列比較** を `insta` snapshot ですでに持っていれば再生成、無ければ本フェーズで追加
- **ログ/ファイル名系 (`profiles/*` / `run_with/*` / `flowgraph/*` / `tts` / `screenshot.rs`) の strftime 結果**: 既存と文字列一致するかを unit test で担保
- **TTL 比較 (`dictionary.rs::is_expired`)**: 現行の文字列 → chrono parse → chrono::Utc::now() 比較のロジックを、Timestamp 比較に書き換え。semantics が変わらないかケース test を追加

### π-3 — chore(deps): chrono 解除

- `Cargo.toml` から `chrono` 行を削除
- `cargo tree | rg chrono` で間接依存を確認。残存する間接依存があれば phase doc に記載 (現時点では直接依存のみの見込み)
- `cargo build --all-features` / `cargo test --lib` / `cargo clippy -- -D warnings` 全 green
- バイナリサイズ比較 (optional): π-1 前と π-3 後の release build `.exe` サイズ / `cargo bloat --release` 抜粋を phase doc に記録

### π-4 — feat(flowgraph/datetime): 型基盤

- `crates/vac-core/src/datetime.rs` に `DateTime(pub Timestamp)` newtype + 変換関数 + エラー型（root `src/datetime/mod.rs` は互換再エクスポート）
- `src/flowgraph/node.rs`:
  - `SocketType::DateTime` variant 追加
  - `SocketValue::DateTime(DateTime)` variant 追加
  - `SocketType::compatible_with` に String ↔ DateTime ルール追加
  - `coerce_to_type` に String ↔ DateTime 実装 (§3.6 naive policy 込み)
- `crates/vac-flowgraph/src/config.rs`（root 側は `src/flowgraph/config.rs` で再エクスポート）:
  - `FlowgraphInstanceConfig.default_timezone: Option<String>` 追加 + load-time 検証
  - `InstanceContext` から engine の coerce 関数へ TZ 情報を渡す配管
- **test**:
  - SocketType round-trip (serialize / deserialize)
  - coerce String → DateTime (aware / naive / error cases)
  - coerce DateTime → String (RFC3339 形式 snapshot)
  - config parse (valid offset / invalid IANA / empty)

### π-5 — feat(flowgraph/nodes/datetime): 8 ノード — [x] 完了

- `src/flowgraph/nodes/datetime.rs` 新設
- 8 ノード (§4) 実装 + `registry.rs` / `nodes/mod.rs` 登録 + `get_required_datetime` (`node.rs`)
- `default_registry_contains_core_features` に 8 feature 追加
- unit test **30** 本（§7.3 の網羅 + 境界）
- `node-catalog.md` 再生成 (`BLESS_NODE_CATALOG=1 cargo test`)

### π-6 — docs — [x] 完了

- `CHANGELOG.md` の `[Unreleased]` に `### π: DateTime Type System` 節を追加
- `docs/manual/datetime-system.md` 新設（動機 / DateTime vs `Quantity<time>` / ノード一覧 / conf / FAQ）
- `docs/manual/index.md` 目次 + Socket 型表に `datetime` を追記
- `docs/roadmap/phase-omicron-flowgraph-enhancement.md`: §3.4 / §5.4 / §5.5 / §6.4 を π-5 後の前提に更新
- `docs/roadmap.md` の Phase π / Phase ο-4 の tick・文言更新

---

## 7. テスト戦略

### 7.1 chrono → jiff 互換テスト (π-2)

- **serde snapshot**: `insta::assert_json_snapshot!` で struct 出力を固定化。置換前にスナップ取得 → 置換後に diff 差分確認 → 差異あれば strftime で形合わせ
- **文字列一致テスト**: ファイル名向け `"%Y%m%d-%H%M%S"` は完全一致。RFC3339 系は正規表現 (`^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$`) で両方 match することを確認
- **TTL 境界テスト**: `dictionary.rs::is_expired` を現行 (chrono) / 新 (jiff) で同一入力に対して同一結果を返すことを property-like test (5 〜 10 ケース) で確認

### 7.2 DateTime socket test (π-4)

- parse: aware / naive / invalid / require_timezone=true で naive 拒否
- format: RFC3339 / custom strftime / TZ 変換
- coerce: String ↔ DateTime の round-trip 精度
- config default tz: "+09:00" / "Z" / invalid / missing

### 7.3 ノードテスト (π-5)

- 各 8 ノードの compute
- `add_duration` / `sub_duration` は次元不一致 (Quantity<length> 渡す等) で error
- `diff` は ns 精度が保たれる
- `from_epoch_ms` は Quantity<time> / Float 両方受ける (暗黙 coerce 経由)

---

## 8. リスクと対策

| リスク | 対策 |
|---|---|
| 21 ファイル一括移行で中間状態が長期間赤 | π-2 はファイル単位で commit、各 commit 後に `cargo check` 必須 |
| serde 出力形式が微妙に変わって外部連携が壊れる | §5 末尾の snapshot test / 正規表現 test を π-2 で先行配備 |
| `jiff` API が `chrono` とずれていて想定外の書き換え | π-1 smoke test を拡張して代表 API の挙動を事前確認 |
| `dictionary.rs::is_expired` のロジック書き換えで TTL semantics が変わる | 境界値テストを先に書く (TDD 寄り) |
| Windows の system timezone 取得が jiff で失敗 | `Zoned::now()` 失敗時は UTC fallback + warn ログ (既存 chrono::Local も実質同様の挙動) |
| MSRV 衝突 | `jiff` の MSRV を π-1 で確認 (本 doc 執筆時点: rust 1.70+ 見込み)、必要なら `rust-toolchain.toml` 調整 |
| naive datetime の曖昧さ (タイムゾーン未指定時の意図ミス) | `require_timezone=true` で strict モード提供、既定は config default tz で warn ログ |

---

## 9. 後方互換性

- **wire format (JSON / TOML)**: 既存の RFC3339 形式 (`"2026-04-24T12:34:56Z"`) は無修正で受理される。subsecond 精度 (`...56.789Z`) も同様
- **Config**: `default_timezone` は未設定時 UTC なので、既存 config 設定ファイルはそのまま動く
- **Socket value**: 既存の `SocketValue::String` に RFC3339 を入れているフロー (例: ο-4 前提の timer 的用法) は engine coerce で自動 DateTime 化される。明示的に String のままにしたければ port 型を `String` に指定
- **Breaking change**: なし (chrono → jiff は内部実装変更、public API 非変更)

---

## 10. 次フェーズへの受け渡し

- **ο-4**: `flowgraph.util.timer_interval` 単独に縮減。timer は DateTime を使わず、engine clock tick + interval 管理で独立実装 (Phase π では扱わない)
- **ρ (OSC/VMC, 旧 π)**: VMC / OSC のタイムスタンプ要素が出てきたら `DateTime` 型で受ける
- **ξ-5 (GUI)**: Phase ξ-5 の unit バッジ対応に合わせて、DateTime socket には専用アイコン (時計マーク) と hover tooltip (`"UTC 2026-04-24T..."`) を追加検討 (phase ξ-5 scope 内で一緒に扱う)
- **π+ (将来)**: IANA tzdb 対応 / `Span` (calendar-aware 暦幅) 導入 / locale-aware format / Celsius/Fahrenheit のような affine unit との統合 (温度 delta と DateTime diff の思想的類似性あり)

---

## 11. 参考

- [jiff crate (crates.io)](https://crates.io/crates/jiff)
- [jiff docs](https://docs.rs/jiff/latest/jiff/)
- [BurntSushi: jiff design rationale](https://github.com/BurntSushi/jiff/blob/master/DESIGN.md)
- [TC39 Temporal proposal](https://tc39.es/proposal-temporal/docs/)
- RFC 3339 / ISO 8601
- [`phase-ksi-dimensional-quantity-system.md`](phase-ksi-dimensional-quantity-system.md) §3 (Quantity`<time>` との接続設計の下敷き)
