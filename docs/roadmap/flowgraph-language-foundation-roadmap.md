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
外部 I/O mock、trigger sequence、expected assertion は LF-3b 以降で追加する。

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
