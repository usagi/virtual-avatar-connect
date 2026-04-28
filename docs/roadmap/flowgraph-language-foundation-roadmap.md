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

## 5. 追加計画項目

以下は重要だが、詳細設計は必要になった段階で起こす。

### LF-4 Module / Package System

`library_uses` を manifest / lockfile / semver / compatibility policy へ発展させる。
標準ライブラリー、ユーザーライブラリー、VAC API ライブラリーを配布・固定・依存解決できるようにする。

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

- [ ] LF-1 Schema / Contract
- [ ] LF-2 Capability / Effect
- [ ] LF-3 Testing / Debugger
- [ ] LF-4 Module / Package System
- [ ] LF-5 Generic / Type Parameter
- [ ] LF-6 Error Model
- [ ] LF-7 Persistence / State Model
- [ ] LF-8 Documentation Generation

