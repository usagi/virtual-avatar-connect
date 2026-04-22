# Phase η — Dictionary / Table Unification

> VAC Flowgraph 上の「辞書」機能を、汎用 `Table` 型と 4 種の `flowgraph.dictionary.*` ノード群で完全再構築するフェーズ。
> V0 `modify` → V1 `dictionary` で合意済みだった仕様（11 カラム辞書・runtime 学習/忘却・TSV 統一）が V2 δ-4a 移行時に失われた経緯を踏まえ、今度こそ文書として固定したうえで実装する。

---

## 1. Background

### 1.1 V0 → V1 → V2 の経緯

- **V0**: `modify` プロセッサが「`dictionary.txt` の逐次 `String::replace`」と「`regex.txt` の正規表現置換」を別プロセッサとして持っていた。ファイル形式は一貫性なく、行末改行・空白区切りのみ。
- **V1**: `dictionary` / `regex` プロセッサに分離しつつ、「先頭 2 トークンのみ有効」という**パース戦略の合理化**が行われた（空白でも TAB でも同じコードで読める）。同時に Control API (`/modify/*`) 経由で runtime 編集可能にする GUI 辞書編集器 (`γ-8`) の構想が走っていた。**このタイミングで 11 カラム辞書・学習/忘却の UNDO 動作・期限切れ・ロック・タグなどの詳細仕様が対話ログ上で合意済みだった**が、正式な markdown 仕様書化はされなかった。
- **V2 δ-4a**: V1 `Modify` / `DictionaryCommand` を Flowgraph の PureNode に移植する際、「Pure なので file I/O はしない」「state 保持は node の外に追い出す」という δ の基本方針に従い、**最小実装**に留まった。その結果:
  - 辞書エントリは `{"to", "from"}` の 2 フィールドに縮退
  - `dictionary.command` ノードが「パーサ + ミューテータ」融合のまま孤立
  - file I/O (`file.read_lines` / `literal.list_json` 相当) が未実装のため、そもそも辞書を graph に供給する手段がない
  - GUI 辞書編集器 (`γ-8`) は `δ-9`（V1 撤去）時に完全削除
  - **結果として本番 flowgraph から辞書機能が実質使えなくなった**（`flowgraph.local/voice-to-tts/main.flowgraph.toml` に `TODO: dictionary.* の内容を List<Json> リテラル として接続する（未実装のため空辞書）` の痕跡が残る）。

### 1.2 なぜ失われたのか

1. 詳細仕様が対話ログ上にしか存在せず、markdown 仕様書に固定されなかった
2. δ-4a の「Pure 優先」方針が「file I/O を伴う辞書ロード」と非整合
3. `γ-8` GUI 実装が V1 撤去に巻き込まれて削除
4. `phase-delta-spec.md §10.2` に「TSV 化（epsilon 枠）」の 1 行だけ残ったが、ε フェーズ側から参照が無くなって孤児化

### 1.3 η の目標

失われた仕様を markdown で完全文書化し、**V2 Flowgraph に「再配管」** する。V1 より綺麗な抽象レベル（Table 型 + 4 ノード + 汎用 I/O）で復活させる。

---

## 2. Goals / Non-Goals

### 2.1 Goals

- V2 Flowgraph 上で辞書の **学習 / 忘却 / 置換 / 照合** が宣言的に組める
- 辞書の **runtime 編集**（Twitch チャンネルポイント、Control API、Quick-Add Widget）
- **TSV 統一フォーマット**（headerful、値内タブは `\t` エスケープ、UTF-8）
- V1 loose 2 列フォーマット（`from to` / `from\tto`）との **後方互換**
- 辞書以外にも **再利用可能な汎用 Table 型**（scene registry、credential store、Twitch user list、moderation log 等の将来ユースケースを下支え）
- 11 カラム辞書スキーマの全機能（期限切れ、優先度、ロック、タグ、note）を動作させる
- Aho-Corasick / RegexSet によるマッチ高速化。辞書不変時は **O(1) キャッシュ再利用**

### 2.2 Non-Goals（η では扱わない）

- 複数辞書を**内部で合成**する Super-Dictionary 型（flowgraph 上で `table.merge` を書けば済む）
- 辞書の分散同期・クラウド共有
- 全文検索・類似語展開など ML 的な機能
- TOML / YAML などの代替ファイル形式の**プライマリ化**（TSV のみを推奨、JSONL は auxiliary）

---

## 3. Data Model — 11 カラム辞書スキーマ

### 3.1 カラム定義

| # | 列名 | 型 | null 可 | 説明 |
|---|---|---|---|---|
| 1 | `source` | string | × | 検索パターン。`kind=literal` なら検索文字列、`kind=regex` なら正規表現 |
| 2 | `replacement` | string | × | 置換先文字列 / Match 時のラベル。regex の場合 `$1` `$2` などキャプチャ参照可 |
| 3 | `kind` | string | × | `literal` / `regex` / (将来: `aho_corasick_group`)。デフォルト `literal` |
| 4 | `priority` | int | × | 高いほど先にマッチ/置換。デフォルト `0` |
| 5 | `is_locked` | bool | × | `true` は Forget で削除不可（arknights 固定辞書など）。デフォルト `false` |
| 6 | `enabled` | bool | × | `false` は Replace/Match で無視。デフォルト `true` |
| 7 | `by` | string | × | 登録者 ID（Twitch username / `system` / `arknights` / `user:<name>` など）。デフォルト `""` |
| 8 | `created_at` | string (RFC3339 UTC) | × | 追加/更新時刻。Learn 時は現在時刻、TSV ロード時はファイルの値を尊重 |
| 9 | `expires_at` | string? (RFC3339 UTC) | ◯ | 期限。`null` なら無期限。過ぎたエントリは Replace/Match の compile 時点で除外 |
| 10 | `tags` | string | × | 自由タグ（CSV 記法 `nsfw,meme,arknights`）。デフォルト `""` |
| 11 | `note` | string | × | 自由メモ。UI の tooltip で表示。デフォルト `""` |

### 3.2 動作ルール

- **重複許容**: 同一 `source` の重複エントリを**許容**。Learn は常に append（既存を書き換えない）。
- **1 件選出**: Replace/Match が 1 件だけ選ぶときは以下の順でソート:
  1. `priority desc`
  2. `created_at desc`（**新しいほうが勝つ** → 学習が古い登録を上書きしたように見える）
  3. 配列順（安定ソート）
- **UNDO 動作**: Forget (mode=`latest`) は `source` 一致の最新 1 件のみ削除 → 以前の登録が自動復活。
- **全削除**: Forget (mode=`all`) は `source` 一致を全削除。
- **ロック尊重**: `is_locked=true` のエントリは Forget から除外。削除対象に含まれていた場合は feedback で件数を報告。
- **期限切れ**: `expires_at` が現在時刻より過去なら `enabled=false` と同様に扱う（compile 時点で除外）。境界は inclusive（`expires_at` ちょうどは expired）。
- **無効化**: `enabled=false` は compile 時点で除外（Replace/Match 対象にならないが Table 上は残る）。
- **literal vs regex の優先**: 同 priority で literal と regex が競合したら **literal 優先**（より具体的なマッチとみなす）。priority が異なる場合は priority を尊重。

### 3.3 行の論理 ID

DB 的な主キーは持たないが、UNDO / runtime 編集の identity として `(source, replacement, kind, created_at)` の 4 つ組を「論理 ID」とする。Forget (mode=`exact`) はこれで完全一致削除。

---

## 4. File Formats

### 4.1 プライマリ: TSV (`.dict.tsv`)

```
source	replacement	kind	priority	is_locked	enabled	by	created_at	expires_at	tags	note
いかく	異格	literal	0	true	true	arknights	2024-01-01T00:00:00Z		arknights	
テスト	test	literal	10	false	true	user:sample	2026-04-23T12:00:00Z	2026-05-01T00:00:00Z	tmp	1週間限定
^(\d+)円$	$1yen	regex	50	false	true	system	2026-04-23T12:00:00Z			通貨正規化
```

- タブ区切り、UTF-8、1 行目はヘッダ（11 カラム固定名）
- 値内タブは `\t`、改行は `\n`、バックスラッシュは `\\` でエスケープ（csv crate 依存せず自前最小実装 or csv crate w/ tab delimiter）
- 空 `expires_at` は空文字列（`null` 扱い）
- 空行は無視、`#` 始まりコメントは無視

### 4.2 V1 loose 互換（`load_tsv` の `mode=legacy_loose` ）

- ヘッダ行なし。先頭 2 トークン（空白でも TAB でも可）を `(source, replacement)` として採用、残余は無視。
- デフォルト値: `kind=literal`, `priority=0`, `is_locked=true`（ファイル由来は固定扱い）, `enabled=true`, `by=<ファイル名ベース>`, `created_at=<ファイルの mtime>`, `expires_at=null`, `tags=""`, `note=""`
- `dictionary.arknights.txt` / `dictionary.pre-coeiroink.txt` / `dictionary.chat.txt` / `dictionary.local.txt` はこのモードで既存のまま読める

### 4.3 JSONL（auxiliary: `.dict.jsonl`）

1 行 1 JSON object、全 11 カラム必須。エクスポート専用。GUI 編集・外部ツール連携用途。
```json
{"source":"いかく","replacement":"異格","kind":"literal","priority":0,"is_locked":true,"enabled":true,"by":"arknights","created_at":"2024-01-01T00:00:00Z","expires_at":null,"tags":"arknights","note":""}
```

---

## 5. Type System Extension — `SocketType::Table`

### 5.1 enum variant 追加

```rust
// src/flowgraph/socket.rs
pub enum SocketType {
    Bool, Int, Float, String, Json,
    List(Box<SocketType>),
    Map(Box<SocketType>),
    Exec,
    Table,   // ← η-1 で追加
}
```

- parser: `"table"` → `SocketType::Table`
- display: `SocketType::Table` → `"table"`
- default_value: `SocketValue::Table(Table::empty())`
- serde: 既存同様に文字列シリアライズ

### 5.2 `Table` データ構造

```rust
// src/flowgraph/table.rs (新設)
use std::sync::Arc;
use std::cell::OnceCell;   // or once_cell::sync::OnceCell
use blake3::Hash;

#[derive(Debug, Clone)]
pub struct Table {
    inner: Arc<TableInner>,
}

pub(crate) struct TableInner {
    pub schema: TableSchema,
    pub rows: Vec<Row>,
    pub version: u64,
    pub content_hash: OnceCell<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema { pub columns: Vec<ColumnSpec> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSpec { pub name: String, pub ty: SocketType, pub nullable: bool }

#[derive(Debug, Clone, PartialEq)]
pub struct Row(pub Vec<serde_json::Value>);   // JSON 値で統一（SocketValue より serde 相性◎）
```

- **Clone は O(1)**（Arc 参照カウントインクリメント）
- **Mutation は COW**: `Arc::make_mut` で `TableInner` を clone し `version` を bump
- **content_hash**: lazy 計算、`(schema + rows)` を canonical JSON serialize → blake3。Stateful ノードの cache key 用途。
- Arc identity と version/hash の 2 段階比較: まず `Arc::ptr_eq` → 同一なら即 reuse、不一致なら hash 比較

### 5.3 `SocketValue::Table`

```rust
pub enum SocketValue {
    // 既存...
    Table(Table),
}
```

- `type_of()` → `SocketType::Table`
- `matches(&SocketType::Table)` → `true`
- JSON への serialize は `to_json` 経由で `List<Map<Json>>` にマッピング（`schema` は失われるが往復ロスレスでなくて良い）

### 5.4 `from_toml_value` 対応

TOML からの復元は現状用途がない（Table リテラルを TOML で書きたいユースは少ない）ので、最小対応として `(SocketType::Table, toml::Value::Array(_))` を `List<Map>` → Table 変換するヘルパに委ねる。

---

## 6. Node Catalog

### 6.1 `flowgraph.table.*`（汎用、η-2 で実装）

| feature | trait | 説明 |
|---|---|---|
| `flowgraph.table.from_json` | Pure | `List<Map<Json>>` or `List<Json>` → Table（スキーマ推論 or 明示） |
| `flowgraph.table.to_json` | Pure | Table → `List<Map<Json>>` |
| `flowgraph.table.load_tsv` | Effectful | path → Table（ヘッダあり/loose 自動判別） |
| `flowgraph.table.write_tsv` | Effectful | Table + path → exec_out |
| (将来) `flowgraph.table.filter` / `.sort` / `.select` / `.append_row` | Pure | η-2 では必要最小のみ |

#### 6.1.1 `flowgraph.table.load_tsv`

- 入力: `exec_in` (Exec), `path` (String), `mode` (String, default `"auto"`)  — `auto` / `headerful` / `legacy_loose`
- 出力: `on_success` / `on_error` (Exec), `table` (Table), `row_count` (Int), `error` (String)
- Effectful: `tokio::fs::read_to_string` + 自前パース
- legacy_loose のときは 11 カラム相当にデフォルト補完（§4.2）

#### 6.1.2 `flowgraph.table.write_tsv`

- 入力: `exec_in` (Exec), `table` (Table), `path` (String)
- 出力: `on_success` / `on_error` (Exec), `bytes_written` (Int), `error` (String)
- 書き込みは **atomic-ish**（`.tmp` にフル書き込み → `rename`）

#### 6.1.3 `flowgraph.table.from_json`

- 入力: `exec_in` (Exec, optional), `json` (List<Json>)
- 出力: `table` (Table), `row_count` (Int)
- スキーマは最初の non-empty row から推論（全カラム `SocketType::Json` として保存）
- Pure なので毎回新しい Table を返す

#### 6.1.4 `flowgraph.table.to_json`

- 入力: `table` (Table)
- 出力: `json` (List<Json>)

### 6.2 `flowgraph.dictionary.*`（η-3 で実装）

| feature | trait | 説明 |
|---|---|---|
| `flowgraph.dictionary.replace` | Stateful | literal(AC) + regex(Regex) 統合置換 |
| `flowgraph.dictionary.match` | Stateful | 照合 + captures + exec 分岐 |
| `flowgraph.dictionary.learn` | Pure | 11 カラム append |
| `flowgraph.dictionary.forget` | Pure | 論理 ID or source ベースで削除（mode, is_locked 尊重） |

#### 6.2.1 `flowgraph.dictionary.replace`

- **Stateful**（AC/Regex キャッシュ）
- 入力:
  - `exec_in` (Exec, optional — pull 型も許容)
  - `content` (String)
  - `dictionary` (Table)
- 出力:
  - `on_done` (Exec)
  - `result` (String)
  - `applied_count` (Int) — 実際に置換が発生した回数の総和
- state: `DictionaryReplaceState { last_version: u64?, last_hash: [u8;32]?, compiled: Arc<CompiledDictionary>? }`
- アルゴリズム:
  1. `dictionary` の Arc identity → version → content_hash の順でキャッシュ判定
  2. miss なら `(priority desc, created_at desc)` でソートして:
     - `enabled && !expired && kind=literal` を Aho-Corasick に入れる
     - `enabled && !expired && kind=regex` を `Regex` 個別コンパイル、`RegexSet` で高速判定
  3. 実行: AC でマッチ集合を取得 → 位置 conflict があれば priority/長さで解決 → 一括置換
  4. regex は AC 後の結果に対して pass 2 で適用（仕様上 regex は literal の後）
  5. `kind` 越しの優先は literal-first。同 kind 内は priority/長さで解決。

#### 6.2.2 `flowgraph.dictionary.match`

- **Stateful**（同じく AC/Regex キャッシュ）
- 入力:
  - `exec_in` (Exec)
  - `text` (String)
  - `dictionary` (Table)
- プロパティ:
  - `match_policy`: `"first"` | `"all"` | `"longest"`（default: `"first"`）
  - `anchor`: `"anywhere"` | `"prefix"` | `"full"`（default: `"anywhere"`）
- 出力:
  - `on_match` (Exec)
  - `on_no_match` (Exec)
  - `matched_entries` (List<Json>) — マッチした行の全カラム（match_policy に応じて件数）
  - `matched_count` (Int)
  - `captures` (List<List<String>>) — `[entry_i][group_j]`、literal は空 Vec
  - `first_replacement` (String) — match_policy=first 時の便利ショートカット（未マッチなら空）
- 挙動:
  - `anchor=anywhere`: 普通のマッチング
  - `anchor=prefix`: 入力先頭に位置するマッチのみ
  - `anchor=full`: 入力全体に一致するマッチのみ（`^...$` を強制）
  - `match_policy=first`: priority/longest で 1 件
  - `match_policy=all`: マッチ全部（位置順）
  - `match_policy=longest`: 入力のある位置から始まる最長マッチ（lexer 用）

#### 6.2.3 `flowgraph.dictionary.learn`

- **Pure**
- 入力:
  - `exec_in` (Exec)
  - `dictionary` (Table)
  - `source` (String)
  - `replacement` (String)
  - `kind` (String, default `"literal"`)
  - `priority` (Int, default `0`)
  - `by` (String, default `""`)
  - `tags` (String, default `""`)
  - `note` (String, default `""`)
  - `expires_at` (String, default `""`) — 空なら null
- 出力:
  - `on_learned` (Exec)
  - `on_duplicate` (Exec) — **完全に同じ論理 ID** のエントリが既にあって append しなかったとき
  - `updated_dictionary` (Table)
  - `added_entry` (Json)
  - `feedback` (String)
- 動作: `created_at = now_utc()`、`is_locked=false`、`enabled=true` で append。重複検出は `(source, replacement, kind)` が完全一致 **かつ** 既存が not expired かつ enabled な場合のみ no-op。

#### 6.2.4 `flowgraph.dictionary.forget`

- **Pure**
- 入力:
  - `exec_in` (Exec)
  - `dictionary` (Table)
  - `source` (String)
  - `replacement` (String, optional default `""`) — 空なら source のみで削除
  - `mode` (String, default `"latest"`) — `latest` / `all` / `exact`
- 出力:
  - `on_forgotten` (Exec)
  - `on_nothing` (Exec)
  - `on_locked` (Exec) — ロックされたエントリが対象に含まれ、削除できなかった件が 1 つでもあるとき
  - `updated_dictionary` (Table)
  - `removed_count` (Int)
  - `locked_count` (Int)
  - `feedback` (String)
- 動作:
  - `mode=latest`: source 一致 (and 指定があれば replacement 一致) の最新 1 件削除
  - `mode=all`: source 一致 (and replacement 一致) 全削除
  - `mode=exact`: `(source, replacement, kind)` 全一致のみ（replacement 必須）
  - `is_locked=true` はすべて除外カウント

### 6.3 削除するノード

- `flowgraph.dictionary.command` — 完全に `dictionary.match` で置換可能。η-4 で削除し、`flowgraph.example/dictionary/command-dispatch.flowgraph.toml` に移行例を置く。

---

## 7. Caching & Performance

### 7.1 Stateful Replace / Match の state

```rust
struct DictionaryCompileState {
    last_arc_ptr: Option<*const TableInner>,  // Arc::as_ptr() で取得、identity 比較用
    last_version: Option<u64>,
    last_hash: Option<[u8; 32]>,
    compiled: Option<Arc<CompiledDictionary>>,
}

struct CompiledDictionary {
    // literal
    ac: Option<aho_corasick::AhoCorasick>,
    literal_replacements: Vec<String>,      // ac pattern_id → replacement
    literal_priorities: Vec<i64>,
    literal_sources: Vec<String>,           // for captures output (=source itself)
    // regex
    regex_set: Option<regex::RegexSet>,
    regex_individual: Vec<regex::Regex>,
    regex_replacements: Vec<String>,
    regex_priorities: Vec<i64>,
    regex_sources: Vec<String>,
    // meta
    entries_count: usize,
}
```

### 7.2 キャッシュ判定フロー

```
input_table を取得
  ↓
Arc::as_ptr(&input.inner) == state.last_arc_ptr ?
  ├─ Yes → compiled を即 reuse（最速）
  └─ No  → input.inner.version == state.last_version ?
           ├─ Yes (同値 Arc が複数経由) → reuse
           └─ No  → input.content_hash() == state.last_hash ?
                   ├─ Yes → reuse（別 Arc だが内容同一）
                   └─ No  → recompile、state を新しい identity/version/hash/compiled で更新
```

### 7.3 コスト見積

- 典型辞書サイズ: 500 エントリ × 30 バイト = 15KB
- blake3 hash: ~μs オーダー
- AC compile: 500 pattern → ~1-3ms（DFA 最小化含む）
- Regex compile: `RegexSet::new` + 個別 `Regex::new` で 1 pattern あたり ~10μs
- キャッシュヒット時: **ポインタ比較のみで μs 未満**
- TTS パイプラインの 1 発話/sec レベルでは完全に誤差

---

## 8. Migration

### 8.1 V1 ファイル → η TSV への変換 CLI（η-5 実装済）

独立 bin `virtual-avatar-connect-migrate-dict`（`src/bin/migrate_dict.rs`）で提供。`clap` ベース、`--input` は複数回指定可、`path` / `path:kind` / `path:kind:tag` 記法をサポート。

```sh
# 固定辞書（Arknights 固有語彙）
virtual-avatar-connect-migrate-dict \
  --input dictionary.arknights.txt:literal \
  --by arknights --locked \
  --output dictionary.arknights.dict.tsv

# pre-coeiroink（TTS 前処理用、同じくロック）
virtual-avatar-connect-migrate-dict \
  --input dictionary.pre-coeiroink.txt:literal \
  --by pre-coeiroink --locked \
  --output dictionary.pre-coeiroink.dict.tsv

# ユーザー学習分（locked 無し）
virtual-avatar-connect-migrate-dict \
  --input dictionary.local.txt:literal \
  --by local \
  --output dictionary.local.dict.tsv

# chat 用辞書 + regex を 1 本に merge
virtual-avatar-connect-migrate-dict \
  --input dictionary.chat.txt:literal:chat \
  --input regex.chat.csv:regex:chat \
  --by chat \
  --output dictionary.chat.dict.tsv

# システムコマンド regex（ロック）
virtual-avatar-connect-migrate-dict \
  --input regex.pre-command.txt:regex:pre-command \
  --by pre-command --locked \
  --output regex.pre-command.dict.tsv
```

実装の要点:

- `--input path:kind:tag` で個別指定可能。`kind = auto / literal / regex`。`auto` は `regex.*.csv` / `regex.*.txt` → `regex`、`dictionary.*.txt` → `literal` を推論。
- `literal` 入力は V1 loose 形式（空白または TAB で `source replacement`、コメント行は `#`）。
- `regex` (space) 入力は `replacement pattern` で、replacement に空白を含むケース（`/set init ^...`）を「空白 + `^` 始まり」のパターンで境界推定して正しく分離。
- `regex` (csv) 入力は標準 CSV `replacement,pattern`（空 replacement 許容、クォート対応）。
- 出力は 11 カラム TSV ヘッダあり、エスケープは `\\` / `\t` / `\n` / `\r`、`expires_at` は空欄。`created_at` はコマンド実行時刻（RFC3339 UTC 秒精度）。
- 既定では既存 output を上書きしない（`--force` で強制）。一時ファイル書き込み → rename で atomic 化。
- `tags` 列には入力ファイルの stem（`dictionary.chat` 等）を記録し、`note` に `migrated from <path>` を残す。
- 学習/忘却ログ的な重複排除はせず、そのまま append（Learn/Forget の UNDO 動作を維持するため運用側で validate）。

### 8.2 既存 conf/flowgraph の更新

- `flowgraph.local/voice-to-tts/main.flowgraph.toml` に `dictionary.arknights.dict.tsv` を `flowgraph.table.load_tsv` で読み込み、`flowgraph.dictionary.replace` に接続する既定配線を追加
- 古い 2 列 `.txt` は deprecated 警告付きで残す（1 リリースあと削除）

---

## 9. GUI

### 9.1 Flowgraph Editor（η-6 で実装済）

- **Table ポートの視覚区別**: `gui/src/lib/flowgraph/FlowgraphNodeCard.svelte` の `handleClass` に
  `port.ty === 'table'` 判定を追加し、Handle に `flowgraph-handle data table` クラスを付与。
  スタイルは「12×12px の角丸正方形、`rgb(16 185 129)` (emerald-500)」で、通常のデータポート（青円）と
  視覚的にはっきり区別できる。exec ポート（橙三角）との三種の形状コーディングが成立。
- **Dictionary カテゴリ/Table カテゴリのパレット表示**: `flowgraph.dictionary.*` と
  `flowgraph.table.*` は node descriptor の `category` で `"dictionary"` / `"table"` を返すため、
  `FlowgraphPalette` の `groupedCatalog()` が自動的に独立セクションとして表示する（追加実装不要）。

### 9.2 Dictionary Editor pane（保留、別 PR）

- 11 カラム表示のテーブル UI（sortable / filterable）
- `is_locked` は UI でも保護（削除不可、警告トースト）
- 検索（source 部分一致、tag 絞り込み、kind filter）
- 期限切れ行はグレーアウト
- **保留理由**: V2 では辞書 Table はファイル I/O が Flowgraph の `flowgraph.table.load_tsv` /
  `write_tsv` ノード経由で完結している。GUI から直接編集するには Flowgraph worker と別系統の
  「現在ディスク上にある Table を読む/書く」Control API が必要になり、η のスコープを超える。
  φ フェーズ以降で「管理対象 TSV ファイルレジストリ」を整備した上で着手する。

### 9.3 Live Quick-Add Widget（保留、別 PR）

- Twitch チャンネルポイント連携用の小型 UI
- 「学習 (source → replacement)」を 1 クリックで Flowgraph の Learn ノードに trigger 投入
- 過去の Quick-Add 履歴と UNDO（Forget latest）ボタン
- **保留理由**: V1 の `DictionaryQuickAddWidget.svelte` は `modify` processor 前提で書かれており、
  V2 では dead code 化している。V2 で同等機能を実装するには Flowgraph の `dictionary.learn` /
  `dictionary.forget` ノードに外部 trigger を打ち込む Control API（`POST /api/v1/control/flowgraph/trigger/{node_id}`
  相当）が必要。こちらも φ 以降。

---

## 10. Test Plan

### 10.1 ユニットテスト（Rust `cargo test`）

- `socket.rs`: `SocketType::Table` parse/display/serde roundtrip
- `table.rs`:
  - Clone は Arc 共有
  - `make_mut` で version bump
  - content_hash が値同一 Arc でキャッシュされる
- `flowgraph/nodes/table/*.rs`:
  - `from_json` → `to_json` roundtrip
  - `load_tsv` headerful / legacy_loose / mixed の 3 モード
  - `write_tsv` 往復
- `flowgraph/nodes/dictionary/*.rs`:
  - Replace: literal-only、regex-only、mixed、priority 順序、expires_at 除外、enabled=false 除外
  - Match: first / all / longest, anchor= {anywhere, prefix, full}, captures
  - Learn: duplicate detect, created_at 自動、priority/tags/note 通過
  - Forget: mode=latest で UNDO 復活、mode=all 全消し、is_locked 保護、on_locked 発火

### 10.2 プロパティベース

- Replace は idempotent な場合（辞書内容同じ）何度適用しても state が変わらない
- Learn → Forget (latest) で辞書が**元に戻る**（UNDO invariant）
- Learn → Learn → Forget で 1 つ残る

### 10.3 Example flowgraph 実走

- `flowgraph.example/dictionary/command-dispatch.flowgraph.toml` を実 load + 1-shot 実行
- `flowgraph.example/dictionary/basic-replace.flowgraph.toml` を実 load + 1-shot 実行
- これらを `cargo test` 内で smoke test 化

---

## 11. Open Questions / Future Extensions

- **`aho_corasick_group` kind**: 複数 source を 1 エントリにまとめる「グループ辞書」。現状 11 カラムに押し込めにくいので ν 以降で別途。
- **タグベース subset**: Replace の property `only_tags: "arknights,jp"` で limiting（η では見送り）
- **辞書の永続化 auto-save**: Learn/Forget の結果を自動 write_tsv で保存するフロー例を `flowgraph.example/dictionary/` に置く（runtime 実装側の追加はなし）
- **Regex kind の $group 展開**: 現状 `regex::Regex::replace` 内蔵の置換機能に委ねる（η ではシンプルに）
- **ν 以降**: Super-Dictionary（辞書の辞書）、Trie ベース辞書、クラウド同期

---

## 12. References

- 合意ログ元: agent transcript `b25a03c5-2a2a-4ead-aa8e-449ede71b4b0.jsonl`（V1 時代の仕様対話）
- 旧実装: `src/flowgraph/nodes/dictionary.rs`（V2 δ-4a 時点、η で分割再編）
- 関連仕様: `docs/roadmap/phase-delta-spec.md` §2.1（型システム）、§3.4（Node 3 分類）、§10.2（TSV 化 — η に委譲）
- V1 loose フォーマット例: `dictionary.arknights.txt`, `dictionary.pre-coeiroink.txt`
