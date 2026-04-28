# VAC Flowgraph Language Roadmap

> Status: accepted design draft for PR review. 本書は VAC Flowgraph を「常駐型汎用データフロー処理エンジン」および「汎用プログラミング言語に近い実行記述」へ進化させるための上位計画である。個別実装フェーズの正本ではなく、既存 Phase δ / λ / ξ / π / ο / M / ρ... を接続する設計判断メモとして扱う。§8 の Decisions は実装時の前提として扱う。

---

## 0. 結論

VAC はすでに、単なる配信補助ツールではなく **常駐型データフロー処理エンジン**として成立し始めている。Flowgraph はそのエンジン上で動く **プログラム**であり、`.flowgraph.toml` は source format、GUI は editor / IDE、Control API は runtime control / debugger に相当する。

今後の主眼は「ノード数をただ増やす」ことではない。汎用言語へ近づけるには、以下を順に固める必要がある。

1. Flowgraph の言語仕様を明文化する
2. サブグラフを関数・ライブラリとして再利用できるようにする
3. 反復・コレクション処理を安全な形で導入する
4. `json` 逃げを減らし、第一級型を増やす
5. エラー・状態・権限・デバッグをランタイム機能として揃える

---

## 1. 現状の到達点

### 1.1 Engine

`src/flowgraph/engine.rs` / `src/flowgraph/node.rs` は、すでにプログラミング言語処理系の中核に近い。

- `PureNode` / `StatefulNode` / `EffectfulNode` による副作用分類
- exec DAG と data DAG の分離
- data 側の pull-demand lazy evaluation
- generation 単位の memoization
- `TriggerHandle` / `TriggerEvent` による常駐 event loop
- external ingress / self ingress / Control API trigger の統一経路

これは、UE Blueprint 型の exec graph と、純粋関数型言語に近い lazy data graph を混ぜた実行モデルである。

### 1.2 Type System

`src/flowgraph/socket.rs` は、すでに primitive の列挙を超えている。

- `bool` / `int` / `float` / `string` / `json`
- `list<T>` / `map<T>`
- `table`
- `quantity`
- `datetime`
- `exec`

さらに `Quantity` と `DateTime` では、限定的な暗黙 coerce と実行時 validation がある。これは「型は事故を防ぐ砦」という VAC の設計思想を言語仕様に持ち込んだもの。

### 1.3 Libraries

Phase λ で以下が入っている。

- `PortSpec.closed_string_variants`
- user-defined `[[enums]]`
- `flowgraph.library.input` / `flowgraph.library.output`
- `[meta]` 拡張
- `library_uses`
- dependency cycle detection

ただし現状の library boundary は v0 stub であり、関数・モジュール・パッケージとして使える段階ではない。

### 1.4 Runtime Position

VAC 本体は `flowgraph_dir` をロードし、Flowgraph worker を常駐 task として起動する。VMC / motion 計画でも、VAC を常駐型 hub として再定義している。つまり Flowgraph は「設定ファイル」ではなく、VAC 常駐ランタイムへロードされる **プログラム単位**である。

---

## 2. 目標モデル

### 2.1 VAC の再定義

```text
VAC =
  always-on dataflow runtime
  + typed Flowgraph programming environment
  + avatar / stream / AI / motion integration hub
```

常駐型であることが重要。CLI 一回実行の変換器ではなく、外部イベント、タイマー、Twitch、voice、VMC、GUI 操作を受け続け、状態を持ち、必要に応じて外部 I/O を行う。

### 2.2 Flowgraph の再定義

```text
VAC Flowgraph =
  typed node-based programming language
  + visual editor friendly source format
  + explicit effect boundary
  + event driven execution model
```

Flowgraph は「ノードを並べた設定」ではなく、以下を備えるプログラムとして扱う。

- source file: `*.flowgraph.toml`
- module / package: directory + `[meta]`
- function: graph-as-node / library signature
- type: `SocketType` + future schema metadata
- effect: `PureNode` / `StatefulNode` / `EffectfulNode`
- runtime: `FlowgraphProgram` worker
- debugger: Control API + GUI diagnostics + future trace tools

### 2.3 非目標

Flowgraph を Rust / Python / JavaScript の代替にしない。VAC が必要とする領域は「常駐イベント処理」「外部アプリ連携」「ストリーム・アバター制御」「AI / motion / UI の glue」であり、一般計算すべてをテキスト言語並みに表現する必要はない。

目指すべきは、以下のような領域で十分な表現力を持つ言語。

- event routing
- stream processing
- stateful reaction
- typed transformation
- external I/O orchestration
- physical / audio / motion computation
- user-editable automation

---

## 3. 設計原則

### 3.1 Safety First

危険な外部操作は `EffectfulNode` に閉じる。Pure / Stateful の実装には `ExecCtx` を渡さない現方針を維持する。

将来の `http` / `process` / `window` / `file` / `osc` / `obs` / `system` 系ノードは、capability policy を持つべきである。少なくとも以下を区別する。

- local-only harmless
- local file read/write
- network access
- process control
- window control
- account / token using API
- destructive operation

### 3.2 Explicit Boundaries

便利な暗黙変換は限定する。`Float ↔ Quantity` や `String ↔ DateTime` のように、目的が明確で診断可能なものだけにする。

`json` は escape hatch として残すが、言語の中核型を `json` に依存させすぎない。

### 3.3 Bounded Generality

反復や再帰は強力だが、常駐ランタイムでは runaway の危険が高い。最初に導入する制御構造は bounded であるべき。

- `list.map`
- `list.filter`
- `list.reduce`
- `table.filter`
- `table.select`
- `table.map_rows`
- bounded repeat

無制限 loop / recursive graph call は後回し。

### 3.4 Editor First, Not Editor Only

GUI で自然に扱えることを重視する。ただし source format は手で読める必要がある。GUI 専用の hidden state で意味論を作らない。

`.flowgraph.toml` は program source、GUI はその editor。

### 3.5 Runtime Observability

常駐エンジンでは、実行中に何が起きているか見えることが言語機能の一部になる。

- load diagnostics
- runtime errors
- node trace
- last value watch
- trigger history
- state snapshot
- reload diff
- performance counters

---

## 4. Language Gaps

### 4.1 Abstraction Gap

現状の library boundary は単一 `string` の stub。汎用言語に近づけるには、サブグラフを「関数」として呼べる必要がある。

必要な機能:

- dynamic boundary ports
- typed signature
- graph-as-node instantiation
- input default values
- output contract
- exec boundary
- nested library dependency
- version / compatibility metadata

### 4.2 Control Gap

現在の制御は `branch` / `gate` / `sequence` / state / timer が中心。反復やコレクション変換が弱い。

必要な機能:

- list / table iteration
- bounded repeat
- map/filter/reduce
- join / group / aggregate
- event windowing
- debounce / throttle 以上の stream operator

### 4.3 Type Gap

`Quantity` / `DateTime` は良い方向。だがまだ多くが `json` に逃げている。

今後導入する第一級型:

- `bytes`
- `record`
- `result`
- `option`
- `toml`
- `motion_frame`
- `osc_packet`
- `http_response`
- `window_handle`
- `process_handle`
- `audio_buffer`

すべてを一気に入れない。最初に `result` / `bytes` / `record` を基礎型として導入する。
`toml` は JSON と同じ構造化データの別表現として扱い、Flowgraph source format / conf / file I/O との親和性が高い場合に第一級型へ昇格させる。

### 4.4 Error Gap

現状は以下が混在する。

- `NodeExecError` として engine error
- `on_error` exec + `error: string`
- warning diagnostics
- log trace

汎用ランタイム化するなら、recoverable error と fatal error の規約をそろえる必要がある。

採用する方針:

- validation error: load 時に止める
- recoverable runtime error: `on_error` / `Result` 出力
- fatal runtime error: worker diagnostics に積むが process は落とさない
- programmer error: test / debug で即検出

### 4.5 State Gap

`StatefulNode` の state は worker 内メモリ。reload / restart / profile switch を跨ぐ永続状態は別設計が必要。

必要な機能:

- graph-scoped key-value state
- typed state slot
- snapshot
- import / export
- reload migration
- state reset policy
- profile-local vs global scope

### 4.6 Package Gap

`library_uses` は依存グラフの始まりだが、パッケージとしては不足がある。

必要な機能:

- public signature
- semantic version
- compatibility range
- examples
- tests
- docs metadata
- lockfile or resolved dependency record

### 4.6.1 Library Layer Gap

Flowgraph を汎用言語へ近づけるには、単にノードを増やすのではなく、機能を以下の層へ分ける必要がある。

- **言語コア**: 型、exec / data 意味論、effect boundary、capability、診断、エラー規約。
- **標準ライブラリー**: file / convert / table / list / math / constants / datetime など、VAC 固有ではない汎用ノード。
- **VAC API ライブラリー**: Runtime Mode、desktop notification、OBS、Twitch、OSC / VMC など、VAC が常駐 hub として提供する外部連携。

この分類を持たないままノードを増やすと、GUI catalog と docs が散らかり、ユーザーにとって「何を使えばよいか」が見えにくくなる。Resident I/O の file / convert / table 系は標準ライブラリー、notify / OBS template 系は VAC API ライブラリーとして扱う。
乱数、分布、ring buffer、sort / search / index などの汎用アルゴリズムは [`flowgraph-stdlib-roadmap.md`](flowgraph-stdlib-roadmap.md) へ分離し、標準ライブラリーとして段階整備する。

### 4.7 Debug Gap

GUI editor はあるが、runtime debugger としてはまだ弱い。

必要な機能:

- run graph once with fixture input
- trigger selected node with inputs
- watch port values
- inspect last N triggers
- step exec chain
- show data pull tree
- show effect boundary
- export trace bundle

---

## 5. Proposed Stages

### Stage 0 — Language Positioning

目的: VAC Flowgraph を「プログラム」として扱う上位方針を文書化する。

成果物:

- 本書
- `roadmap.md` から本書への入口
- `architecture.md` に反映する場合は、本書承認後の別 PR で「Flowgraph as Program Runtime」節として扱う

実装は行わない。

### Stage 1 — Language Spec Consolidation

目的: 既存仕様を再整理し、Flowgraph の言語仕様として読める文書を作る。

対象:

- execution model
- type model
- effect model
- file/module model
- diagnostics model
- reload model
- compatibility policy

既存の `phase-delta-spec.md` は基礎仕様として残す。統合視点は新しい開発者向け正本文書 `docs/roadmap/flowgraph-language-spec.md` に集約し、ユーザー向け短縮版は必要になった段階で `docs/manual/flowgraph-language.md` として派生させる。

採用する文書構成:

- `docs/roadmap/flowgraph-language-spec.md`: 開発者向け正本
- `docs/manual/flowgraph-language.md`: ユーザー向け短縮版（language spec が固まった後に作成）

### Stage 2 — λ+ Graph-as-Node

目的: サブグラフを関数・ライブラリとして呼べるようにする。

主な設計:

- `flowgraph.library.input` / `output` の dynamic ports
- boundary node を source of truth として signature を生成
- `[meta.signature]` は将来の cache / public API 表現として生成・検証対象にする
- graph-as-node catalog entry
- library instance properties
- exec input / output boundary
- internal node id namespace
- diagnostic mapping from instance back to source graph

最初のスコープ:

- 1 file = 1 library function
- acyclic dependency only
- no recursion
- no dynamic type parameter
- no variadic ports

実装単位:

1. docs: λ+ detailed spec
2. loader: signature extraction
3. engine: subprogram instance node
4. GUI: catalog entry + boundary editor
5. tests: sample library + graph-as-node execution

### Stage 3 — Collection Processing

目的: 反復を安全な関数型コレクション操作として導入する。

採用する初期ノード群:

- `flowgraph.list.map`
- `flowgraph.list.filter`
- `flowgraph.list.reduce`
- `flowgraph.table.filter`
- `flowgraph.table.select`
- `flowgraph.table.map_rows`
- `flowgraph.table.aggregate`

実装上の制約:

- ノードがサブグラフを引数として取る必要がある
- `map` には callback graph signature が必要
- callback graph は Stage 3 v0 では Pure only に制限する
- Effectful callback は禁止する
- Stateful callback は Stage 3 v0 では禁止し、必要になった段階で persistent graph state との整合を取って別途解禁する

実行保護:

- max_items / timeout / cancellation を持つ

### Stage 4 — Result / Error Type

目的: recoverable error を String + `on_error` から第一級値へ段階移行する。

採用する型:

```text
result<T>
  ok: bool
  value: T?
  error: string?
  code: string?
```

実装上は `SocketType::Result(Box<SocketType>)` として表現する。`value` の型は `T`、`error` / `code` は `string`、`ok` は `bool` とする。

最初の対象:

- HTTP
- JSON parse
- DateTime parse
- unit parse
- Twitch / OBS / OSC I/O

互換性:

- 既存 `on_success` / `on_error` は残す
- data 出力として `result` を追加
- GUI は Result port を専用表示

### Stage 5 — Record / Schema Type

目的: `json` の乱用を減らし、構造データを型付きで扱う。

採用する型表現:

- named schema in `[types]`
- `record<schema_id>` を socket type 表記として使う
- table row schema は named schema を参照して再利用する

最初の用途:

- VMC `MotionFrame`
- OSC packet
- HTTP response
- Twitch event
- AI tool call payload

方針:

- まずは named schema metadata + runtime validation
- full structural type equality は後回し
- GUI 表示と docs generator を優先

### Stage 6 — Persistent Graph State

目的: 常駐ランタイムとして reload / restart を跨ぐ state を扱う。

採用する初期ノード群:

- `flowgraph.state.kv_get`
- `flowgraph.state.kv_set`
- `flowgraph.state.kv_delete`
- `flowgraph.state.snapshot`
- `flowgraph.state.restore`

採用する設計:

- profile-local state path
- graph fq name と state key namespace
- version migration
- secret と通常 state の分離
- crash-safe write

保存形式は既存 `state_data_path` との整合を優先して決める。Stage 6 の設計文書で、既存永続化との互換性を確認したうえで wire format を確定する。

### Stage 7 — Capability Model

目的: 汎用外部 I/O ノードを安全に増やす。

対象:

- `flowgraph.http.request`
- `flowgraph.file.*`
- `flowgraph.process.*`
- `flowgraph.window.*`
- `flowgraph.obs.*`
- `flowgraph.osc.*`
- `flowgraph.system.*`

最初に必要なもの:

- capability declaration in node specs
- config-level allow/deny
- GUI warning
- LAN control policyとの整合
- destructive operation confirmation

採用する config 形:

```toml
[flowgraph.capabilities]
network = "allow_local_only"
file_read = ["./data", "./flowgraph"]
file_write = ["./data"]
process = "deny"
window_control = "deny"
```

### Stage 8 — Debugger / Test Runner

目的: Flowgraph をプログラムとして保守できるテスト・デバッグ基盤を作る。

採用する初期機能:

- `vac flowgraph test`
- `*.flowgraph.test.toml`
- fixture trigger
- expected trace
- expected channel output
- expected state diff
- expected diagnostics

GUI:

- selected node trigger
- watch port
- trigger timeline
- last error panel
- data pull tree viewer

---

## 6. Priority

### P0: Now

- 本書を承認可能な設計文書にする
- `roadmap.md` に本書への入口を追加
- `architecture.md` への反映は別 PR に分離する

### P1: Next Design

- Flowgraph Language Spec
- λ+ graph-as-node detailed spec
- Debug/test runner の最小仕様

### P2: First Implementation Tracks

Cursor 側の M 系や GUI 作業と衝突しにくい初期実装トラック:

- docs-only language spec
- loader-level library signature extraction
- Flowgraph fixture test runner
- `Result` 型の設計のみ
- node catalog metadata 拡張

### P3: Larger Implementation

- graph-as-node runtime
- collection higher-order nodes
- persistent graph state
- capability model

---

## 7. Interaction with Existing Phases

### Phase δ

δ は基礎仕様。今後も「Flowgraph の最小意味論」の参照元として残す。本書は δ を置き換えない。

### Phase λ

λ は enum / library v0。汎用言語化の最初の本丸は λ+ である。dynamic boundary ports と graph-as-node はここに接続する。

### Phase ξ / π

Quantity / DateTime は型システム拡張の成功例。今後の `record` / `result` / `bytes` / `motion_frame` も、同じように「事故を型で防ぐ」方針で設計する。

### Phase ο

math / vec / signal util / random / noise / timer は計算ライブラリとしての基盤。collection / physics / audio-reactive はここから伸びる。

### Phase M

M は VAC を常駐型 motion hub として外へ広げる実用線。`MotionFrame` は record/schema 型導入の有力な最初の利用者。

### Phase ρ / σ / τ / υ / ω

これらは外部 I/O、GUI、audio/physics など個別領域の拡張。言語化ロードマップは横串として、capability、debug、type、package の共通設計を与える。

---

## 8. Decisions

### 8.1 Dynamic Boundary Port Source of Truth

Library signature は boundary node の property から生成する。v0 では boundary node が source of truth であり、GUI は boundary node を編集対象にする。

`[meta.signature]` は v0 の手書き source of truth にはしない。将来の cache / public API 表現として生成し、存在する場合は loader が boundary node 由来の signature と整合検証する。

### 8.2 Graph-as-Node and State

ライブラリ内 StatefulNode の state は、呼び出しインスタンスごとに分離する。graph-as-node の各 instance は instance-local state を持つ。shared state が必要な場合は、Stage 6 の persistent graph state を明示的に使う。

### 8.3 Effectful Subgraphs

Effectful node を含む library も graph-as-node 化できる。ただし graph-as-node 自体の effect class は内包ノードから推論する。

- pure only -> Pure
- stateful included, no effectful -> Stateful
- effectful included -> Effectful

これにより `PureNode` から effectful graph を pull できない現行安全性を保てる。

### 8.4 Higher-order Graphs

`list.map` が callback graph を受け取ると、Flowgraph は高階関数を持つことになる。

Stage 3 では callback graph を pure-only に制限する。Effectful iteration は明示的な bounded exec loop として別扱いにする。
標準ライブラリー側ではこれを Ranges / LINQ / Iterator foundation として扱い、list / table / stream の query pipeline を先に整える（[`flowgraph-stdlib-roadmap.md`](flowgraph-stdlib-roadmap.md) SL-0）。

### 8.5 Text Language

TOML 以外のテキスト DSL は当面導入しない。TOML source + GUI editor + fragment ZIP の整理を優先する。DSL は graph-as-node / signature / schema が固まった後の別フェーズで扱う。

### 8.6 Compatibility

Flowgraph source format の互換性は以下で保証する。

- load-time warning for deprecated feature
- migration tool where possible
- node feature rename table
- `schema_version` は必要になった時点で `[meta]` に追加

---

## 9. Suggested First PR Series

### PR 1: docs-flowgraph-language-roadmap

Scope:

- 本書
- `roadmap.md` link

No code.

### PR 2: docs-flowgraph-language-spec

Scope:

- execution / type / effect / file model を統合した language spec draft
- existing docs への cross-link

No code.

### PR 3: docs-lambda-plus-graph-as-node

Scope:

- λ+ detailed spec
- dynamic boundary port design
- graph-as-node implementation plan
- tests plan

No code.

### PR 4: test-flowgraph-fixture-runner

Scope:

- 最小 lib test helper
- existing examples を fixture として実行
- no GUI changes

Cursor 側の GUI / motion 実装と衝突しにくい。

---

## 10. Acceptance Criteria for This Roadmap

本書が承認される条件:

- VAC を常駐型汎用データフローエンジンとして扱う方針が明確
- Flowgraph をプログラムとして扱う用語が明確
- 既存 Phase との衝突がない
- 実装順が「抽象化 → 反復 → 型 → state/error/debug/capability」の形で読み取れる
- すぐ実装しない大物が、後続フェーズとして隔離されている
