# Flowgraph Language Foundation Roadmap

VAC Flowgraph を汎用プログラミング言語に近づけるための基礎整備計画。
標準ライブラリーや外部 I/O を増やす前に、型契約、権限、検証、デバッグを言語機能として揃える。

## 1. 優先順位

最優先は以下の 3 本。

1. **Schema / Contract**
2. **Capability / Effect**
3. **Testing / Debugger**

これらは標準ライブラリー、Resident I/O、Glossary rename、SQLite、OBS template のすべてに影響する。
ここが曖昧なまま機能だけを増やすと、ノード数は増えても言語としての一貫性が崩れる。

## 2. LF-1 Schema / Contract

`record`, `table schema`, node signature, library signature を統一する契約システム。

### 目的

- `json` 逃げを減らす
- Table / SQLite / Google Sheets / JSON / TOML / OBS template を同じ schema 語彙で扱う
- Graph-as-node の input / output contract を検証できるようにする
- GUI が port / row / record の構造を理解し、補完・警告・フォーム生成できるようにする

### 対象

- `[types]` による named record schema
- `table<schema>` または Table metadata による row schema
- node signature の machine-readable 化
- library signature の生成・検証
- schema migration / compatibility rule

### 初期実装方針

- v0 は named `record` と Table column schema の明文化から始める
- runtime 値は既存 `serde_json::Value` / `Table` を活かし、schema validation を外側に足す
- GUI は schema を読んで入力フォームとエラー表示を強化する

### LF-1a node catalog contract metadata ✅

初段として、`GET /flowgraph/node-catalog` の各 node spec に派生 `contract` metadata を追加する。
既存の `inputs` / `outputs` / `properties` は互換維持し、`contract` は今後の library signature / GUI 補完 / schema validation が読む machine-readable 入口とする。

`contract` v1:

- `kind = "node"`
- `feature`
- `inputs[]` / `outputs[]`: `name`, `type`, `direction`, `exec`, `optional`, `multi`, `default`, `enum`
- `properties[]`: `name`, `type`, `default`, `required`, `validator`, `enum`
- `summary`: port / property 数、exec 入出力の有無

この段階では validation enforcement は行わない。既存 `NodeSpec` からの派生値として出し、後続で named `record` / `table schema` / library signature に接続する。

## 3. LF-2 Capability / Effect

Pure / Stateful / Effectful の大分類に加えて、具体的な権限と effect kind を扱う。

### 目的

- file / process / network / db / notification / OBS / Twitch を安全に扱う
- Flowgraph 単位、library 単位、node 単位で必要権限を見える化する
- GUI が危険な操作を事前に表示し、ユーザーが理解して許可できるようにする
- 常駐アプリとして mode ごとの許可・抑制を扱えるようにする

### effect kind 候補

- `file_read`
- `file_write`
- `file_watch`
- `network`
- `db_read`
- `db_write`
- `process_control`
- `window_control`
- `desktop_notification`
- `obs_control`
- `twitch_api`
- `credential_access`

### 初期実装方針

- node catalog に `effects` / `capabilities` metadata を追加する
- loader は graph 全体の required capability summary を生成する
- GUI は graph load 時と node palette で effect を表示する
- policy enforcement は read-only diagnostics から始め、後続で hard deny / mode policy を導入する

### LF-2a node catalog effect metadata ✅

初段として、`GET /flowgraph/node-catalog` の各 node spec に以下を追加する。

- `effect_class`: `pure` / `stateful` / `effectful`
- `capabilities`: node が要求する capability の配列

`capabilities` は現時点では feature / category からの保守的な推定であり、policy enforcement は行わない。
GUI 表示、graph capability summary、fixture test の mock capability 設計に使うための read-only metadata とする。

### LF-2b GUI effect visibility ✅

Node Palette と Flowgraph Node Card に `effect_class` と capability summary を表示する。
ユーザーは node を追加する前後で、Pure / Stateful / Effectful と外部 I/O の種別を確認できる。
これは hard permission UI ではなく、LF-2a metadata の可視化である。

### LF-2c graph capability summary ✅

`LoadReport` に graph 全体の `capability_summary` を追加した。
各 node の feature / effect_class / capabilities を集約し、graph が要求する capability 群を read-only metadata として得られるようにする。

```json
{
  "node_count": 8,
  "effectful_node_count": 3,
  "capabilities": ["file_read", "file_write", "network"],
  "nodes": [
    {
      "node": "main::load",
      "feature": "flowgraph.table.load_tsv",
      "effect_class": "effectful",
      "capabilities": ["file_read"]
    }
  ]
}
```

`GET /flowgraph/diagnostics` でも同じ summary を返す。
現時点では policy enforcement ではなく、GUI 表示、Runtime Mode / Capability policy preview、fixture mock 設計のための足場。

## 4. LF-3 Testing / Debugger

Flowgraph を「プログラム」として扱うための検証と観測。

### 目的

- graph 単位の fixture test を書けるようにする
- node / subgraph / library の入出力を再現可能に検証する
- external I/O を mock capability で差し替える
- 常駐 runtime の trigger / data pull / effect boundary を追跡する

### Testing

- `*.flowgraph.test.toml` または `[tests]` section
- fixture input / expected output
- expected diagnostics
- mock file / network / db / obs / twitch
- deterministic time / random seed
- CI で headless 実行できる test runner

### Debugger

- run graph once with fixture input
- trigger selected node with inputs
- watch port values
- inspect last N triggers
- step exec chain
- show data pull tree
- show effect boundary
- export trace bundle

### 初期実装方針

- まず CLI test runner と trace JSON export
- 次に GUI の watch / trigger history / data pull tree
- E2E は GUI 操作ではなく Flowgraph runtime の言語テストを主にする

### LF-3a CLI fixture runner / trace JSON ✅

既存の `flowgraph::fixture_runner` を CLI から呼べるようにし、Flowgraph ディレクトリを 1-shot 実行して summary / trace を出力する。

```powershell
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-dir flowgraph.example/lambda-demo
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-dir flowgraph.example/lambda-demo --flowgraph-test-json
```

初期版は `FlowgraphProgram::execute()` による 1-shot 実行のみを扱う。
出力は `generation`, `node_count`, `trace`, `stored_values`, `exec_count`, `pure_evaluations`, `cache_hits`, `cache_misses`。
外部 I/O mock、trigger sequence、より高度な expected assertion は LF-3b 以降で追加する。

### LF-3b `*.flowgraph.test.toml` minimal assertions ✅

Flowgraph ディレクトリ直下の `*.flowgraph.test.toml` を読み、`[[tests]]` の最小 assertion を評価する。

```toml
[[tests]]
name = "one-shot smoke"

[tests.expect]
node_count = 3
trace_count = 0
trace = []

[[tests.expect.stored_values]]
node = "node_id"
port = "value"
ty = "string"
value = "expected"
```

初期版の assertion は `node_count`, `trace_count`, `trace`, `stored_values[]`。
`stored_values[]` は `node`, `port`, `value` を比較し、任意で `ty` も検証する。
失敗時は CLI が non-zero exit し、JSON 出力では `tests[]` / `failed_tests` に結果を載せる。
trigger sequence、mock capability、近似比較や部分一致などの高度な assertion は次段で追加する。

### LF-3c fixture trigger sequence ✅

`*.flowgraph.test.toml` の `[[triggers]]` で、`run_forever_with_bus` に渡す外部 trigger sequence を宣言できる。
これにより、HTTP / Twitch / Voice などの実ブリッジを起動せずに ingress 型 Flowgraph の exec 経路を検証する。

```toml
[[triggers]]
node = "main::in"
exec = ["__trigger__"] # 省略時は ["__trigger__"]
delay_ms = 0

[[triggers.overrides]]
port = "__content__"
ty = "string"
value = "hello fixture"
```

初期版は trigger を順に投入し、全 trigger の合計遅延 + 短い余白で shutdown する。
`flowgraph.example/twitch-echo` はこの形式で `ingress.twitch -> util.log` を fixture 化済み。
JSON 出力には `trigger_count` と `trigger_history[]` を載せ、投入した node / exec / delay / override を後から確認できる。
`[tests.expect]` では `trigger_count` も検証できる。
mock capability と、GUI 側の trigger history 表示は次段で追加する。

### LF-3d fixture HTTP mock capability ✅

`*.flowgraph.test.toml` の `[mocks]` で、HTTP request ノードの外部 I/O を node 単位に差し替えられる。
初期版は `flowgraph.http.request` の response / error mock に限定し、実ネットワークを叩かずに webhook / REST 連携グラフを検証する。

```toml
[mocks]

[[mocks.http]]
node = "main::http_request"
status = 202
body = '{"accepted":true}'
json = { accepted = true }
```

JSON 出力には `mock_count` と `mocks[]` を載せ、`[tests.expect]` でも `mock_count` を検証できる。
`flowgraph.example/http-webhook` は success mock、`flowgraph.example/http-webhook-error` は error mock として、
`ingress.web_input -> http.request -> util.log` の両分岐を fixture 化済み。
分岐確認には `[[tests.expect.exec_count]]` も使い、想定外の branch が発火していないことを検証する。
mock HTTP は `recorded_effects[]` に method / url / request_body / status / response_body / error を残す。
`[tests.expect]` では `[[tests.expect.http_requests]]` で HTTP request / response 形状を検証できる。
今後は db / obs / twitch などへ同じ `[mocks.<capability>]` 形式で広げる。

### LF-3e fixture file read mock capability ✅

`*.flowgraph.test.toml` の `[mocks]` で、ファイル読み取り I/O も node 単位に差し替えられる。
初期版は `flowgraph.table.load_tsv` の TSV 読み取りに限定し、fixture 内で TSV 本文または読み取り error を宣言する。

```toml
[mocks]

[[mocks.file_read]]
node = "main::load"
contents = """
source	replacement
hello	hi
"""
```

`flowgraph.example/table-load-tsv-mock` は `ingress.web_input -> table.load_tsv` を実ファイルなしで検証する最小 fixture。
`flowgraph.example/table-load-tsv-mock-error` は mock error による `on_error` 分岐と stored error を検証する。
JSON 出力には HTTP mock と同じ `mock_count` / `mocks[]` として `kind = "file_read"` が載る。
mock read は `recorded_effects[]` に path / bytes / contents / error を残し、`[[tests.expect.file_reads]]` で検証できる。
これにより、ファイル監視・辞書・Table 系ノードの regression test を実ファイル配置に依存させずに増やせる。

### LF-3f fixture file write mock capability ✅

`*.flowgraph.test.toml` の `[mocks]` で、ファイル書き込み I/O も node 単位に差し替えられる。
初期版は `flowgraph.table.write_tsv` に限定し、実ファイルを書かずに `bytes_written` と success / error 分岐を検証する。

```toml
[mocks]

[[mocks.file_write]]
node = "main::write"
```

`flowgraph.example/table-write-tsv-mock` は `table.load_tsv -> table.write_tsv` を file read / write の両 mock で検証する。
`flowgraph.example/table-write-tsv-mock-error` は mock error による `on_error` 分岐と stored error を検証する。
JSON 出力には `kind = "file_write"` として載る。
mock write は `recorded_effects[]` に path / bytes / contents / error を残す。
`[tests.expect]` では `effect_count` と `[[tests.expect.file_writes]]` で、書き込まれる本文まで厳密に検証できる。

### LF-3g recorded effects schema ✅

fixture report の `recorded_effects[]` は、mock された外部 I/O を後から検証するための共通 effect log として扱う。

共通フィールド:

- `kind`: `http` / `file_read` / `file_write`
- `node`: node fq id
- `error`: mock error。success 時は `null`

HTTP:

- `method`
- `url`
- `request_body`
- `status`
- `response_body`

File read / write:

- `path`
- `bytes`
- `contents`

`[tests.expect]` では `effect_count` に加えて、kind 別に `[[tests.expect.http_requests]]` / `[[tests.expect.file_reads]]` / `[[tests.expect.file_writes]]` を使う。
これにより trace string の目視依存を減らし、外部 I/O 境界を構造化して regression test できる。

### LF-3h fixture suite runner ✅

`--flowgraph-test-root` で、指定 root 配下の `*.flowgraph.test.toml` を持つ Flowgraph fixture ディレクトリを列挙して一括実行できる。

```powershell
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-root flowgraph.example
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-root flowgraph.example --flowgraph-test-json
```

JSON 出力は `FixtureSuiteReport` として、`fixture_count` / `failed_fixtures` / `reports[]` / `errors[]` を返す。
各 fixture の report は単体 `--flowgraph-test-dir` と同じ `FixtureRunReport`。
これにより、CI やローカル回帰確認で fixture 群をまとめて検証できる。

### LF-3i fixture runner manual ✅

Flowgraph fixture runner の開発者向け手順を `docs/manual/tutorials/flowgraph-fixtures.md` に追加した。
manual では次を正本として扱う。

- 単体 fixture: `--flowgraph-test-dir`
- suite 実行: `--flowgraph-test-root`
- JSON 出力: `--flowgraph-test-json`
- mock IO: `[[mocks.http]]` / `[[mocks.file_read]]` / `[[mocks.file_write]]`
- recorded effects: `http` / `file_read` / `file_write`
- assertion: `effect_count` と `[[tests.expect.*]]`

GUI E2E は画面と Control API の往復、Flowgraph fixture は Flowgraph runtime / language の回帰を見る層として分担する。
CI 化は引き続き optional とし、まずローカルで `--flowgraph-test-root flowgraph.example` を回す運用を標準とする。

### LF-3j trigger history assertions ✅

fixture report の `trigger_history[]` を `[[tests.expect.trigger_history]]` で検証できるようにした。

```toml
[[tests.expect.trigger_history]]
node = "main::in"
exec = ["__trigger__"]
delay_ms = 0

[[tests.expect.trigger_history.overrides]]
port = "__content__"
ty = "string"
value = "hello fixture"
```

`flowgraph.example/twitch-echo` は、投入した ingress trigger の node / exec / override を構造化 assertion で検証する。
これにより、ingress 系 fixture でも trace 文字列への依存を減らし、fixture input の形そのものを regression test できる。

### LF-3k suite summary fields ✅

`FixtureSuiteReport` に suite 直下の集計値を追加した。

- `test_count`
- `failed_tests`
- `trigger_count`
- `effect_count`

通常出力と JSON 出力の要約を揃え、CI やスクリプトが `reports[]` 全体を走査しなくても、suite 全体の規模と失敗数を読めるようにする。

### LF-3l single `[test]` fixture shorthand ✅

単一 test case の fixture では `[[tests]]` ではなく `[test]` を使えるようにした。

```toml
[test]
name = "one-shot smoke"

[test.expect]
node_count = 3
trace_count = 0
trace = []
```

複数ケースは引き続き `[[tests]]` を使う。
`flowgraph.example/lambda-demo` は `[test]` 形式へ移行し、単純な fixture の記述量を減らした。

### LF-3m empty fixture test guard ✅

`*.flowgraph.test.toml` が存在しても `[test]` / `[[tests]]` が 1 件もない場合は失敗扱いにした。
ファイルだけ置かれていて assertion が空の fixture を成功扱いすると、回帰テストとしては静かな空振りになるため。

### LF-3n trace contains assertion ✅

`[test.expect]` / `[tests.expect]` に `trace_contains = ["..."]` を追加した。
`trace = [...]` は完全一致、`trace_contains` は各文字列が trace のどこかに含まれていることだけを検証する。
診断文字列全体を固定したくないが、重要な分岐や error fragment は確認したい fixture に使う。

### LF-3o stored value JSON pointer assertions ✅

`[[tests.expect.stored_value_paths]]` を追加した。
stored value 全体ではなく、JSON Pointer で指定した断片だけを検証できる。

```toml
[[tests.expect.stored_value_paths]]
node = "main::load"
port = "table"
pointer = "/0/source"
value = "hello"
```

Table / JSON の出力全体を固定すると fixture が重くなるため、重要なセルや field だけを pin する用途に使う。

## 5. 追加計画項目

以下は重要だが、詳細設計は必要になった段階で起こす。

### LF-4 Module / Package System

`library_uses` を manifest / lockfile / semver / compatibility policy へ発展させる。
標準ライブラリー、ユーザーライブラリー、VAC API ライブラリーを配布・固定・依存解決できるようにする。

#### WASM compiled module target

WASM は Flowgraph の主表現ではなく、module / package system の実行ターゲットの 1 つとして扱う。

- `.flowgraph.toml`: source / visual editable program
- Graph-as-node: Flowgraph native module
- WASM module: 高速・sandbox・配布可能な compiled module
- VAC API: host function として必要最小限を WASM へ公開する

初期方針:

- LF-1 Schema / Contract と LF-2 Capability / Effect の後に設計する
- 最初は Pure function module 限定
- 次に Stateful module
- Effectful / host API import は capability model が固まってから解禁する
- WASM は GUI で中身を直接編集できない black box module として扱い、signature / docs / trace を必須にする

必要な設計:

- WASM ABI
- Flowgraph 型と WASM value の変換
- schema / signature の同梱形式
- host function import の capability 宣言
- versioning / compatibility / lockfile
- debug symbol / trace metadata

### LF-5 Generic / Type Parameter

`list<T>`, `dictionary<K,V>`, `result<T>`, future `collection<T>` を自然に扱うための型パラメータ。
最初は GUI 上の型束縛と loader validation から始める。

### LF-6 Error Model

`result<T>`, `on_error`, fatal diagnostics, retry policy, fallback を統一する。
Resident I/O と SQLite、Google Sheets、OBS template で必須になる。

### LF-7 Persistence / State Model

StatefulNode の state 寿命、reload 時保持、profile-local/global、snapshot/migration を定義する。
ring buffer、PRNG state、WebSub subscription、mode state で必要。

### LF-8 Documentation Generation

node signature / library signature / schema から manual と GUI catalog を生成する。
ノードが増えた後に手書き docs が破綻しないための基盤。

## 6. 実装順序

- [~] LF-1 Schema / Contract
- [~] LF-2 Capability / Effect
- [~] LF-3 Testing / Debugger
- [ ] LF-4 Module / Package System
- [ ] LF-5 Generic / Type Parameter
- [ ] LF-6 Error Model
- [ ] LF-7 Persistence / State Model
- [ ] LF-8 Documentation Generation
