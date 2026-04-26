# Phase δ Specification — VAC Flowgraph

**Status**: Phase δ-0 locked. 基準仕様として δ-1 以降の実装が参照する。実装中の発見で随時改訂する場合は §14 の合意事項を更新し、変更点を CHANGELOG または PR 説明に記録する。

**Scope**: VAC のパイプライン処理を「型付き IO コネクター + exec pin による DAG」= **VAC Flowgraph** へ全面刷新するための仕様。AI Persona 統合は本フェーズ対象外。V1 処理系は δ 閉幕時点で完全削除し v1.0 に major bump。

---

## 1. 用語

- **Flowgraph** — 本プロジェクトで採用するビジュアルプログラミングモデルの総称。UE Blueprint に影響を受けた、typed socket + exec pin + DAG。
- **Node** — Flowgraph の最小処理単位。入力ポート・出力ポート・プロパティ・実行ロジックを持つ。
- **Port** — ノードの IO 端子。`input` / `output`、`data` / `exec` の直交した 2 分類を持つ。
- **Socket** — ポート間を接続する型付きワイヤ。`SocketType` に対応する値（`SocketValue`）を運ぶ。
- **Edge** — グラフ上の 2 ポート間接続。
- **Exec pin** — 制御フロー専用ポート。データを運ばず「発火」だけを伝える。
- **fq name (fully-qualified name)** — ファイルを跨いだノード参照のための正規化識別子。`folder/sub/file::node_id` 形式。
- **Flowgraph file** — `*.flowgraph.toml`。単一ファイルに複数ノード・エッジを持つ。
- **Flowgraph folder** — `flowgraph/` 以下のサブフォルダ。Rust の module フォルダに相当。
- **Fragment** — 選択範囲（ノード群 / ファイル群 / フォルダ群）を share/export 目的に切り出した TOML or ZIP パッケージ。

---

## 2. 型システム

### 2.1 SocketType

```rust
enum SocketType {
    Bool,
    Int,      // i64
    Float,    // f64
    String,
    Json,     // serde_json::Value 相当
    List(Box<SocketType>),
    Map(Box<SocketType>),   // key 型は常に String（δ-0 合意）。value 型のみ指定
    Table,                  // η: 表形式データ（Columns × Rows）。辞書・scene registry 等で共通利用
    Exec,                   // 制御フロー専用（値なし）
}
```

- **制御フローとデータフローの分離**: `Exec` は値を持たず、ノード発火の有無だけを伝える。他の型は data。
- **型整合**: 接続時は `output.type == input.type` を要求する。`Json` と他型の相互変換は明示ノード（`JsonParse` / `JsonStringify` 等）でのみ行う。
- **List**: ジェネリクス的に `List<String>` / `List<Json>` 等を許容。
- **Map の key 型は常に `String`**（δ-0 合意）: `Map<T>` の表記で value 型のみ指定する。非 String key が必要な場合は `Json` 型で代替。
- **Table**（η 追加）: スキーマ付きの行指向データ。`Arc<TableInner>` ベースの clone O(1)、`Arc::make_mut` による COW mutation。`SocketValue::Table` に対応。`List<Json>` との相互変換は `flowgraph.table.from_json` / `flowgraph.table.to_json` で明示的に行う（暗黙変換は無し）。辞書ノード（`flowgraph.dictionary.*`）は Table を受け取り、内部で AC/Regex キャッシュを content-hash で再利用する。詳細は [phase-eta-dictionary-unification.md](phase-eta-dictionary-unification.md) §3 / §5 を参照。

### 2.2 SocketValue

ランタイム実体。

```rust
enum SocketValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Json(serde_json::Value),
    List(Vec<SocketValue>),
    Map(BTreeMap<String, SocketValue>),  // key は常に String
    // Exec は値を持たないので variant 無し。発火のみ。
}
```

- **暗黙変換は行わない**。`Int → Float` / `String → Int` などが必要な場面は専用ノード or ユーザー明示接続を要求。
- **変換ユーティリティ**（ノード実装内で使用）:
  - `SocketValue::as_bool() -> Result<bool>` / `as_i64()` / `as_f64()` / `as_str()` / `as_json()` / `as_list()` / `as_map()`
  - `SocketValue::from_str(type_hint, "...")` — TOML リテラルから `SocketType` に合わせて値を復元（デフォルト値解釈用）

### 2.3 型表記（spec / TOML 内）

- 原始型: `"bool"` / `"int"` / `"float"` / `"string"` / `"json"` / `"exec"`
- 複合型: `"list<string>"` / `"map<json>"` / `"list<map<string>>"`（ネストは再帰的、Map は value 型のみ指定）
- 互換受理: `"map<string, T>"` 記法も `"map<T>"` と同等に受け入れる（他言語慣習との互換）
- parser は簡易文法でパース。後述 TOML spec に記述。

---

## 3. ノードモデル

### 3.1 NodeSpec（ノード定義）

各ノード実装が静的に宣言するメタデータ。GUI/loader が契約として参照する。

```rust
struct NodeSpec {
    feature: &'static str,            // グローバル一意な識別子（下記規則）
    title: &'static str,              // GUI 表示名
    category: &'static str,           // 分類（"ingress" / "utility.json" / "tts" など）
    description: Option<&'static str>,// 説明（GUI ツールチップ等）
    inputs: &'static [PortSpec],      // 入力ポート（exec/data 混在可）
    outputs: &'static [PortSpec],     // 出力ポート
    properties: &'static [PropertySpec], // インスタンス属性
}
```

**feature 命名規則**: `flowgraph.<category>.<name>` snake_case 推奨。例:

- `flowgraph.ingress.twitch`
- `flowgraph.ingress.web_input`
- `flowgraph.ingress.voice`
- `flowgraph.literal.string`
- `flowgraph.literal.json`
- `flowgraph.flow.branch`
- `flowgraph.flow.sequence`
- `flowgraph.flow.gate`              (δ-2: 純関数版 `exec_in + is_open:Bool → exec_out`)
- `flowgraph.state.bool`             (δ-3: BoolState。`set_true`/`set_false`/`toggle` → `value:Bool` + `changed:exec`)
- `flowgraph.state.int_counter`      (δ-3: カウンター)
- `flowgraph.state.latch`            (δ-3: 任意値の保持セル)
- `flowgraph.util.log`
- `flowgraph.util.delay`             (δ-3: 内部 self-ingress。blocking せず data snapshot + timer 駆動)
- `flowgraph.json.parse`
- `flowgraph.json.stringify`
- `flowgraph.json.get`
- `flowgraph.json.set`
- `flowgraph.string.format`
- `flowgraph.string.concat`
- `flowgraph.regex.match`
- `flowgraph.regex.replace`
- `flowgraph.dictionary.replace`    (η: Stateful、Table 入力、literal+regex 統合、AC キャッシュ)
- `flowgraph.dictionary.match`      (η 新設: Stateful、captures 出力、match_policy / anchor プロパティ)
- `flowgraph.dictionary.learn`      (η 新設: Pure、11 カラム append + 重複検出)
- `flowgraph.dictionary.forget`     (η 新設: Pure、mode=latest/all/exact、is_locked 尊重)
- `flowgraph.table.from_json`       (η 新設: Pure)
- `flowgraph.table.to_json`         (η 新設: Pure)
- `flowgraph.table.load_tsv`        (η 新設: Effectful、auto / headerful / legacy_loose)
- `flowgraph.table.write_tsv`       (η 新設: Effectful、atomic rename)
- ~~`flowgraph.dictionary.command`~~ (η で削除、`dictionary.match` + `dictionary.learn` / `.forget` で代替。詳細は [phase-eta-dictionary-unification.md](phase-eta-dictionary-unification.md) §6)
- `flowgraph.command.match`
- `flowgraph.screenshot.capture`
- `flowgraph.ocr.recognize`
- `flowgraph.translate.gas`
- `flowgraph.translate.libre`
- `flowgraph.tts.os`
- `flowgraph.tts.coeiroink`
- `flowgraph.tts.aivis_speech`
- `flowgraph.tts.voicevox`
- `flowgraph.tts.bouyomichan`
- `flowgraph.twitch.chat_send`

### 3.2 PortSpec（ポート定義）

```rust
struct PortSpec {
    name: &'static str,           // ファイル内の識別子（snake_case 推奨）
    label: &'static str,          // GUI 表示ラベル
    ty: SocketType,               // 型
    direction: PortDirection,     // Input | Output
    is_exec: bool,                // exec pin なら true（ty は SocketType::Exec）
    optional: bool,               // 未接続でも走る
    default: Option<DefaultValue>,// 未接続時の初期値（Exec / 必須ポートには使えない）
    multi: bool,                  // 複数入力の fan-in を受ける
    description: Option<&'static str>,
}

enum PortDirection { Input, Output }
enum DefaultValue { Literal(SocketValue), Empty }
```

**ルール**:

- 同一ノード内でポート `name` は重複不可。
- `is_exec == true` の場合 `ty == SocketType::Exec` 必須。`default` は不可。
- `multi == true` の入力ポートは、同一型複数出力の fan-in をリスト形式で受ける。ノード実装は `Vec<SocketValue>` を受け取る。
- `exec` ポートは出力 1 → 入力 1 接続（分岐したければ `Sequence` ノードを経由）。

### 3.3 PropertySpec（インスタンス属性）

```rust
struct PropertySpec {
    name: &'static str,
    label: &'static str,
    ty: SocketType,               // List/Map/Json を含む任意型
    default: DefaultValue,
    required: bool,
    description: Option<&'static str>,
    validator: Option<PropertyValidator>, // "regex" / "url" / "nonempty" / カスタム
    ui_hint: Option<UiHint>,      // "multiline" / "password" / "file_path" / "dropdown:choices=..."
}
```

- 1 ノードインスタンスごとに値を持つ。GUI は `PropertySpec` から自動フォームを生成。
- **ポートとの違い**: プロパティは「設計時に決めるノードの振る舞い」、ポートは「実行時にグラフから流れてくる値」。ただし将来的に「プロパティをポート化して上流から上書きできる」機能は δ スコープ外として検討余地あり。

### 3.4 Node trait 3 分類（純粋関数型ハイブリッド）

VAC Flowgraph は「**exec 副作用は eager strict / data 計算は pull-demand lazy / state は engine が World として保持**」というハイブリッドモデルを採用する（§5 参照）。これを支えるため、ノード実装 trait を明確に 3 種に分離する:

```rust
/// (1) 純粋関数ノード。副作用なし、状態なし、決定論的。
/// data 出力はメモ化され、未使用なら評価すらされない（pull-demand lazy）。
#[async_trait]
pub trait PureNode: NodeDescriptor + Send + Sync {
    async fn compute(
        &self,
        props: &InputMap,
        inputs: &InputMap,
        fired_exec: &ExecFireSet,
    ) -> Result<NodeOutput, NodeExecError>;
}

/// (2) 状態付きノード。engine が保持する World から自分の state slot を借りて計算。
/// I/O はしない（= `ExecCtx` の I/O 系リソースにはアクセスしない）。
#[async_trait]
pub trait StatefulNode: NodeDescriptor + Send + Sync {
    fn init_state(&self) -> Box<dyn std::any::Any + Send>;
    async fn compute(
        &self,
        state: &mut (dyn std::any::Any + Send),
        props: &InputMap,
        inputs: &InputMap,
        fired_exec: &ExecFireSet,
    ) -> Result<NodeOutput, NodeExecError>;
}

/// (3) 副作用ノード。I/O 可。ExecCtx 経由でリソース (Player / HTTP / Twitch 等) に触れる。
/// 必ず exec 発火経由で評価される。data pull では決して起動しない。
#[async_trait]
pub trait EffectfulNode: NodeDescriptor + Send + Sync {
    async fn execute(
        &self,
        ctx: &mut ExecCtx,
        props: &InputMap,
        inputs: &InputMap,
        fired_exec: &ExecFireSet,
    ) -> Result<NodeOutput, NodeExecError>;
}
```

- **隠蔽された State モナド**: `StatefulNode::compute` は概念的に `(World, Inputs) -> (World', Outputs)`。engine が `HashMap<NodeId, Box<dyn Any>>` として World を所有し、世代（generation）カウンタで pure のキャッシュ不整合を防ぐ。ユーザーは「state ノード」という 1 つの絵を見るだけで、モナドや合成則を知る必要がない。
- **型で純粋性を強制**: `PureNode` は `&mut ExecCtx` を受け取れないので、Player / HTTP / ファイル I/O に触れない（コンパイル時拒否）。逆に `EffectfulNode` は必ず `ExecCtx` 経由なので、副作用は常に明示される。
- **共通 enum**: engine 内部は以下の enum で homogeneous に扱う。

```rust
pub enum NodeImpl {
    Pure(Arc<dyn PureNode>),
    Stateful { node: Arc<dyn StatefulNode>, state: Box<dyn Any + Send> },
    Effectful(Arc<dyn EffectfulNode>),
}
```

- **Flow 制御ノード（Branch / Sequence / Gate 純関数版）は PureNode**: 副作用を起こさず、exec 発火パターンを純粋に決める（`fire_exec` を返す純粋関数）。これにより「pure DAG の中で exec 分岐」も lazy 評価の恩恵を受ける。

### 3.5 ノードカタログと分担（δ-2 / δ-3 / δ-4）

大まかな担当 phase は以下の通り:

| 分類 | trait | δ-2（純関数） | δ-3（Stateful / Ingress / Trigger Source） | δ-4（副作用移植） |
| -- | -- | -- | -- | -- |
| literal | Pure | Bool / Int / Float / String / Json (δ-1 済) | - | - |
| flow | Pure | `branch` (δ-1 済), `sequence` (δ-1 済), `gate` (純関数版) | - | - |
| util | Pure | `string.*`, `json.*`, `list.*`, `map.*`, `logic.*`, `compare.*`, `convert.*` | - | - |
| util (async) | Stateful | - | `util.delay`（内部 self-ingress） | - |
| state | Stateful | - | `state.bool` / `state.int_counter` / `state.latch` / `state.accumulator` | - |
| ingress | Effectful (trigger source) | - | `ingress.web_input` / `ingress.voice` / `ingress.twitch` | - |
| command | Pure | - | - | `command.match` (δ-4a ✅) |
| dictionary | Pure | - | - | `dictionary.replace` / `dictionary.command` (δ-4a ✅) |
| regex | Pure | - | - | `regex.replace` (δ-4a ✅) |
| screenshot | Effectful | - | - | `screenshot.capture` (δ-4b ✅ / Windows) |
| ocr | Effectful | - | - | `ocr.recognize` (δ-4b ✅ / Windows) |
| translate | Effectful | - | - | `translate.gas` / `translate.libre` (δ-4b ✅) |
| tts | Effectful | - | - | **`tts.speak` 1 種（driver registry で 6 エンジン統合: os / bouyomichan / voicevox / aivis_speech / coeiroink / voicepeak） (δ-4c ✅, δ-4c.1 ✅)** |
| side-effect | Effectful | `log` (δ-1 済、最小版) | - | `tts.speak` (δ-4c ✅) / `twitch.{chat_send,validate_token,user_id_by_login,ban,timeout}` (δ-4d ✅) / `util.rate_limit` (δ-4d ✅) |

---

## 4. エッジモデル

### 4.1 EdgeSpec

```rust
struct EdgeSpec {
    from: PortRef,  // 出力ポート
    to:   PortRef,  // 入力ポート
}
struct PortRef { node: FqName, port: String }
```

### 4.2 接続ルール

- `from.port` は `direction == Output`、`to.port` は `direction == Input` であること。
- `from.ty == to.ty` であること（暗黙変換なし）。
- `exec` と `data` は混在不可（両方 `Exec` か両方 data）。
- 循環禁止（exec と data それぞれで DAG）。ただし同一ノードの loop-back や self-edge も禁止。
- `multi == false` の入力ポートには最大 1 本。`multi == true` なら複数可。

---

## 5. 実行モデル（Haskell / UE ハイブリッド）

### 5.1 基本ポリシー

- Flowgraph 全体を nodes + edges の DAG とみなし、**exec エッジと data エッジを別々の DAG** として扱う。
- **exec DAG は eager strict / ordered**（UE Blueprint 流）。副作用・状態遷移・外部 I/O はここを通じて発火する。
- **data DAG は pull-demand lazy / memoized**（Haskell 流）。Pure ノードは「必要になって初めて評価」され、結果は generation 内でメモ化される。未使用の出力は評価すらされない。
- **State は engine が World として保持**（State モナドの隠蔽）。`StatefulNode` は自分の state slot を `&mut` で受け取り、`(world, inputs) -> (world', outputs)` のように状態遷移を表現する。ユーザーは「state ノード」というアイコンを見るだけで、モナド/合成則を知る必要はない。

この 3 層分離により、VAC Flowgraph は:

- UE Blueprint と同じ体感（exec を辿れば実行順が追える）
- Haskell と同じ純粋関数型の恩恵（未使用計算の除去、メモ化、条件付き評価）
- 型による副作用の明示化（PureNode は I/O に触れない）

### 5.2 評価アルゴリズム

**入口（trigger）**:

- **外部 trigger**: Ingress 系 `EffectfulNode`（WebInput / Voice / Twitch）が外部イベントで起動。`tokio::select!` で多重待機。
- **内部 trigger**: `util.delay` 等の `StatefulNode` が `tokio::spawn(sleep)` で自己起動し、目覚めたら engine の trigger キューに自分を投入。
- どの trigger も「対象ノード ID + 発火した exec 入力セット + 入力 data スナップショット」を運ぶ。

**exec 発火時のノード評価**（strict push）:

1. engine は対象ノードの `NodeImpl` を取り出す
2. 必要な data 入力ポートを `pull_port()` で評価（pull-demand lazy）
3. `PureNode::compute` / `StatefulNode::compute` / `EffectfulNode::execute` のいずれかを呼ぶ
4. 戻り値の `fired_exec` を見て、接続された下流 exec 入力ポートを持つノードを evaluation queue に積む
5. キューが尽きるまで 1-4 を繰り返す（同一 exec チェーン上では順序決定論的）

**`pull_port(node, port)` の評価**（demand lazy）:

```rust
fn pull_port(node_id, port_name, generation, ctx) -> SocketValue:
    // 1. そのノードの出力キャッシュを確認
    if let Some(cached) = node.output_cache[port_name].get(generation):
        return cached.clone();

    // 2. ノード種別で分岐
    match node.impl:
        PureNode =>
            // 必須 input port を再帰的に pull（副作用ゼロ）
            for each input_port required:
                inputs[p] = pull_port(upstream, upstream_port, generation, ctx);
            result = PureNode::compute(props, inputs, fired).await?;
            // 全出力をキャッシュ
            for (out_port, val) in result.data:
                node.output_cache[out_port].set(generation, val);
            return inputs[port_name];

        StatefulNode =>
            // pull 経由でも評価される。state が副作用の入れ物。
            // ただし一度 exec で発火した state は次 generation までキャッシュ可
            ...

        EffectfulNode =>
            // pull 経由では評価しない（副作用を意図せず起こさない）
            // → 直近の exec 発火で格納された最新値があればそれを返す、なければエラー
            if let Some(v) = node.output_cache[port_name].get_any():
                return v;
            Err("EffectfulNode output pulled before any exec firing")
```

**Generation（世代）**:

- 外部/内部 trigger が発火するたびに engine は `generation += 1`
- Pure ノードのキャッシュは `(generation, port_name) -> value`
- Stateful ノードの state は保持、ただし cache key に `state.version` も含める（stale cache 防止）
- 副作用ノードは generation 超えでキャッシュ失効（新しい発火で新しい結果）

**Dead port / dead branch elimination**:

- 1 回の exec cycle で「副作用ノードが実際に pull した data ポート」だけが評価される
- Branch の未選択枝の下流 pure ノードは評価されない
- `Log("...").value` が繋がる `StringFormat(a, b, c)` も、Log が exec で発火しなければ `a, b, c` の pull は起きない

**副作用の順序**:

- 同一 exec チェーン上では決定論的（上から下へ strict）
- 並列実行は exec 分岐（`Sequence` の枝別れ等）のみ
- 副作用ノード内で `.await` がある場合、chain は await の解決まで待つ

**エラーハンドリング**:

- ノードが `Err` を返した場合、**その exec chain は中断**、エラーは Evaluation Lineage（「どの trigger 起点のどの exec パス」）とともに `EventBus` に発火
- 他の exec chain / 並行 chain は影響を受けない
- GUI は該当ノードに赤バッジ + エラー詳細表示

**型による副作用保証**:

- `PureNode::compute` は `ExecCtx` を受け取らない（I/O 不可能）
- `StatefulNode::compute` は `ExecCtx` を受け取らないが `&mut dyn Any` で state に書ける
- `EffectfulNode::execute` のみ `&mut ExecCtx` を受け取る（I/O 可能）
- これによりコンパイル時に「純粋性違反」を検出

### 5.3 グラフ構築と検証

- Loader は全ファイルを読み終えた後、**単一の統合グラフ**を構築する（ファイル境界は実行にとっては透過）。
- 検証順序:
  1. fq name 重複 / 未解決参照
  2. ポート存在確認
  3. 型整合
  4. exec/data 整合
  5. 循環検出（data DAG と exec DAG それぞれ）
  6. 必須入力の充足可能性（未接続かつ default なしの必須入力は **error**、起動時エラーとなる。δ-0 合意で安全側に固定）
  7. **`EffectfulNode` 出力の pull 元チェック**: 副作用ノードの data 出力を pull する純関数ノードがある場合、その副作用ノードが exec 発火前に呼ばれるとエラーになるため、warning を出す
- 検証失敗は起動時エラー（CLI）or GUI で赤表示（それでもアプリは起動継続、該当ノードはスキップ）。

### 5.4 Trigger Source と self-ingress（δ-3 で昇格、実装完了）

> **δ-3 実装ノート**（2026-04-17）: 以下の形で実装済み。
>
> - `TriggerEvent { node_id, fired_exec, data_overrides }`: エンジンに「このノードのこれらの exec 入力ポートを、これらの data override 付きで発火せよ」と伝える値。`data_overrides` は 1 回の発火に限定で通常の pull を上書きする。
> - `TriggerHandle` (`tokio::sync::mpsc::UnboundedSender` を包んだ `Clone` 可能なハンドル): `tokio::spawn` したタスクや外部ハンドラがここから engine を叩く。
> - `StatefulCtx<'a> { node_id, trigger: Option<&TriggerHandle> }`: `StatefulNode::compute` に渡される限定 ctx。I/O には触れないが、自己 trigger だけは可能。
> - `ExecCtx` 拡張: `trigger: Option<TriggerHandle>` と `node_id: String` を追加。EffectfulNode は ctx 経由で trigger にアクセス可。
> - `FlowgraphProgram::run_forever(ctx, shutdown)` / `run_forever_with_bus(ctx, handle, rx, shutdown)`: 初回 `execute()` 実行後、`tokio::select!` で shutdown と trigger 受信を多重待機。`run_forever_with_bus` は外部が bus を所有するパターン（Ingress のブリッジから trigger を打ちたいとき用）。
> - `flowgraph.util.delay` (`StatefulNode`): exec_in 発火で `(value, delay_ms)` を state に保存・timer を spawn、timer 完了時に自己 `TriggerEvent` を送信して `__resume__` exec ポートを叩く。複数の並行遅延を id で追跡するので順不同完了にも対応。
> - `flowgraph.state.{bool,int_counter,latch,accumulator}` (`StatefulNode`): 代表的なミニ state machine。`changed` / `updated` / `cleared` exec 出力で状態変化を下流に通知する。
> - `flowgraph.ingress.{web_input,voice,twitch}` (`PureNode`、echo 型): 外部コードが `TriggerEvent` を `TriggerHandle::send` すると `exec_out` が発火し `content` / `source_actor` / `source_kind` / `meta` を下流に流す。V1 の実 I/O との接続は δ-4d で担当。
> - ポート命名規則: `__foo__` は「内部ポート」の命名規則。loader / GUI では hide 前提。δ-5/δ-6 で尊重される。

> **δ-4a 実装ノート**（2026-04-17）: Command / Modify / DictionaryCommand の移植を完了。いずれも PureNode として実装し、V1 が暗黙に持っていた「辞書/正規表現の状態保持」は **ノード外** に押し出した。状態を持ちたい利用者は `state.latch` に辞書リストを保存し、`dictionary.command` の `updated_dictionary` 出力を `state.latch` の input に配線するだけで V1 相当の挙動を再現できる。
>
> - `flowgraph.command.match`: prefix（default `/`）で始まる文字列を分解し、先頭トークンを `command`、残りを `args: List<String>` に展開する。`on_command` / `on_other` の 2 系統 exec 出力で下流に分岐を譲る。V1 の quit/disable/... のようなハードコード dispatch はユーザ側の `compare.eq` + `branch` の連鎖に委ねる。
> - `flowgraph.dictionary.replace`: `dictionary: List<Json>`（各要素 `{"to", "from"}`）で `content: String` を逐次 `String::replace` 置換。ソート順は**ユーザ責任**で、必要なら `state.latch` の初期値側でソート済みリストを投入する（V1 の `sort_dictionary=length` は `list.sort_by_key(len)` 相当のノードで δ-2 の `collection` に積まれている想定。無ければ δ-4c 付近で補強）。
> - `flowgraph.regex.replace`: `rules: List<Json>`（各要素 `{"pattern", "replacement"}`）。コンパイルに失敗したパターンは **スキップして `errors: List<String>` に収集**、致命エラーにはしない（V1 の `regex::Regex::new(&from_matcher)?` が起動時エラーだったのに対し、Flowgraph 側は GUI でのライブ編集を想定して非致命化）。
> - `flowgraph.dictionary.command`: `学習(src:=repl)` / `忘却(src:=repl)` / `忘却(src)` / `learn(...)` / `forget(...)` を V1 DictionaryCommand と同一文法（全角・半角括弧両対応）で解釈。権限判定（`allowed_flags` / `allowed_logins` / `strip_chat_prefix`）は flowgraph 側では **分離**し、ユーザが上流に `flow.gate` / `compare.eq` を組むことで表現する想定（「権限チェックを構造として可視化する」方針）。sender_login の抽出は δ-4d で Twitch ingress が `meta` / `source_actor` 経由で提供する契約。
> - **辞書の永続化**: Pure ノードなのでファイル I/O は一切行わない。ファイルからの初期ロードや書き戻しは、δ-4b 以降で追加する `file.read_lines` / `file.write_lines` のような `EffectfulNode` + `json_ops`（パース/シリアライズ）の組合せで、ユーザ自身が構築するモデル（V1 の `dictionary_files` / `writable_dictionary_file` の自動化は migrate CLI で生成する）。

> **δ-4b 実装ノート**（2026-04-17）: Screenshot / OCR / GasTranslation / LibreTranslation を `EffectfulNode` として移植完了。δ-4a の PureNode 群と異なり、**I/O 失敗を Err で engine を止めるのではなく `on_error` exec + `error: String` で下流に流す**「非致命エラー規約」を本フェーズで確立した。GUI でノードをライブ編集しながらエラーを可視化・分岐できるための設計である。
>
> - `flowgraph.screenshot.capture`: V1 `Screenshot` の Flowgraph 版。Windows 限定の `crate::processor::screenshot::windows`（`capture_display` / `capture_window_ex`）を再利用する。V1 の「複数クロップで一度に複数画像を出す」配列モデルは廃し、**1 発火 = 1 画像**に単純化。多重クロップはユーザが `list.for_each` 相当で展開する（Flowgraph 流の explicit-iteration）。`target_title_regex` > `target_title` > デスクトップ の優先順位、`crop: Json` の 4 軸 Option 指定、`save_path` の `{T}` テンプレート置換、`data_url` 出力（`data:image/png;base64,...`）はいずれも V1 互換。非 Windows では常に `on_error` が発火し「platform not supported」を返す。
> - `flowgraph.ocr.recognize`: V1 `Ocr` の Flowgraph 版。Windows Media OCR を `crate::processor::ocr::recognize` で呼び出す。V1 の `load_from: [url, url, ...]` と `load_from_content: json-array` の 2 系統入力は廃し、**単一 `source: String` に統一**。文字列プレフィックス（`http://` / `https://` / `file:///` / `data:` / それ以外＝ローカルパス）でソース種別を自動判定する。`check_result_lang` は whatlang で OCR 結果を再検査し、`lang` の先頭言語部（`"ja-JP"` → `"ja"`）と一致しなければ `on_error` を発火する（V1 互換）。複数画像を扱いたい場合はユーザ側で `list.for_each` を組む。
> - `flowgraph.translate.gas`: V1 `GasTranslation` の Flowgraph 版。URL テンプレート・`whatlang` による自動言語推定は V1 互換。`script_id` は **入力ポート空文字なら環境変数 `VAC_GAS_TRANSLATION_SCRIPT_ID` を fallback** する形にして、ノード単位で差し替え可能にした（V1 の `ENV or conf` は Flowgraph 層では「conf」の概念が無いので「入力ポート or env」に読み替え）。HTTP エラーは非 2xx でも `on_error` に流す。
> - `flowgraph.translate.libre`: V1 `LibreTranslation` の Flowgraph 版。`base_url` は**必須入力**とし、埋め込みサーバ（V1 の `state.libretranslate.ensure_server`）のライフサイクル管理はノードのスコープ外とした。埋め込みサーバが必要な構成は、将来の `libretranslate.embed.start` Effectful ノードまたは外部ランタイム（systemd / docker 等）に委ねる「単機能の明示化」方針。HTTP 部は既存の `crate::libretranslate::translate` をそのまま再利用。
> - **`whatlang` + `isolang` の 639-3 → 639-1 変換ヘルパ**: translate_gas / translate_libre / ocr で共通。現状は各ノードにコピペしているが、利用拡大時に `flowgraph::nodes::_lang_util` のような private module に寄せる（δ-4c で TTS の言語推定を入れる際にリファクタ予定）。
> - **プラットフォーム依存の再輸出**: `src/processor/mod.rs` で `mod ocr; mod screenshot;` だった 2 モジュールを `pub(crate) mod ocr; pub(crate) mod screenshot;` に昇格させ、Flowgraph 側から OCR / Windows capture 実装を再利用できるようにした。V1 プロセッサ層と並走する過渡期限定の措置で、δ-9 の V1 除去時にトップレベル `src/capture/` / `src/ocr/` への完全プロモーションで解消する。
> - **テスト戦略**: HTTP モックは導入せず、(a) no-fire noop、(b) 入力バリデーションによる `on_error` 発火、(c) 到達不能 URL に対する非致命エラー化、の 3 層でノード固有の契約を確認する。実 HTTP / 実 OCR の結合テストは δ-5（Loader）経由のスモークテストで行う方針。

> **δ-4c 実装ノート**（2026-04-17）: TTS 系 5 種（`os_tts` / `bouyomichan` / `voicevox` / `aivis_speech` / `coeiroink`）を **単一 `flowgraph.tts.speak` EffectfulNode** に統合した（δ-4c.1 で VoicePeak も追加し、計 6 エンジン同梱）。複数ノードを並べず、`engine` 入力で駆動ドライバを動的に切り替える **Driver Registry 設計**（`src/flowgraph/tts/`）を採用。
>
> - **なぜ統合したか**: GUI（δ-6）から見たとき、TTS は本質的に「外部音声エンジン連携」という 1 つの概念である。5 ノード並べる設計はユーザの認知負荷を増やすだけで、エンジン切替（「このキャラだけ VOICEVOX、他は棒読み」）の実現にも逆に graph 書換えが必要になる。統合すれば `engine` 入力を `state.latch` で切り替えるだけでライブ差替えが可能になり、**グラフ構造を変えずにエンジンを替えられる**。
> - **ドライバ抽象 `TtsDriver`**: `async fn speak(&self, req: TtsRequest, audio: &AudioContext<'_>) -> Result<TtsOutcome, TtsError>` の 1 メソッド trait。正規化入力（`speed=1.0`=標準、`pitch=0.0`=標準、`volume=1.0`=標準）とエンジン共通の 3 入力（`voice`, `endpoint`, `save_path`）、および escape hatch の `extra: Map<Json>` を受ける。戻り値は `played: bool` + `audio_path: String` の 2 観測量。
> - **`TtsRegistry`**: `LazyLock<TtsRegistry>` で 1 回だけ初期化する静的レジストリ。VoicePeak 等の新エンジンは `drivers/<name>.rs` に `TtsDriver` 実装を 1 つ足し、`registry::default_registry()` に 1 行追加するだけでノード側の変更ゼロで対応できる。
> - **Bouyomichan の CLI → TCP 移行**: V1 は `RemoteTalk.exe` CLI を spawn していたが、棒読みちゃん公式 **TCP プロトコル（port 50001）をデフォルト化**して外部 exe のパス設定を不要にした。プロトコルは `iCommand(i16) / iSpeed(i16) / iTone(i16) / iVolume(i16) / iVoice(i16) / bCode(u8=UTF-8) / iLength(i32) / bText` の小さなバイナリパケットで、`tokio::net::TcpStream` に 1 発 write すれば再生が始まる。ネットワーク越しの別マシン上の棒読みちゃんにも接続できるようになった副次効果あり。
> - **VOICEVOX 系 2 種の共通化**: `voicevox` と `aivis_speech` は VOICEVOX 互換 HTTP API（`POST /audio_query` → `POST /synthesis`）を使う完全な兄弟関係なので、`drivers/voicevox.rs` に共通実装 `speak_voicevox_like(engine_base_default, req, audio)` を置き、`aivis_speech.rs` は `name()` と既定エンドポイント（`http://127.0.0.1:10101`）だけ差し替えた 10 行程度の薄いラッパにした。合成本体は既存の `crate::processor::voicevox_engine::synthesize_wav` を再利用。
> - **`voice` 入力の意味論統一**: VOICEVOX 系と CoeiroInk は `"speaker_uuid:local_style_index"` 形式、VOICEVOX 系は追加で `"12345"`（グローバル style_id）の直指定も可。Bouyomichan は `"0"`〜`"8"` の数値文字列、OS TTS は音声名の部分一致（V1 互換）。全て **1 つの String ポート** に集約した。
> - **CoeiroInk の簡素化**: V1 は `/v1/predict` と `/v1/synthesis` の両対応を `fix_conf` 内で場当たり的に切り替えていたが、Flowgraph 版では **`/v1/synthesis` のみ**に固定して JSON スキーマも `SynthesisRequest` 1 本に整理。`predict` は使わない（`synthesis` のスーパーセット）ため機能低下はない。V1 の暗黙の設定補完（`speaker_uuid` 未指定時の自動選択等）は **Flowgraph 側の責任ではない**（不足時は明示的エラー）とし、migrate CLI がレガシー TOML からの変換時に固定値として埋め込む方針。
> - **OS TTS の正規化マッピング**: `tts` crate は OS 依存の `min_rate/normal_rate/max_rate` を提供するため、`req.speed` を `[min, normal, max]` の線形補間にマップ（`speed>1 → [normal, max]`、`<1 → [min, normal]`）。pitch/volume も同様。これで Windows / macOS / Linux のエンジン差を吸収しつつ、ユーザは正規化値の意味だけ覚えればよくなる。
> - **`ExecCtx` 拡張 `audio_sink`**: `SharedAudioSink` を `ExecCtx` のフィールドに追加した（`Option<SharedAudioSink>`）。TTS ドライバは `ctx.audio_sink` 経由で VAC 共有の rodio `Player` に WAV を append する。sink 未初期化のテストや headless 実行では自動的に synthesize-only モード（`played=false`）にフォールバックする。Bouyomichan のように自前再生するドライバは sink を使わない。
> - **分割再生の graph-level 化**: V1 の `split_regex_pattern` による 1 発話多文分割はドライバから削除。Flowgraph では `flowgraph.regex.replace` や `flowgraph.string.split`（δ-2 予定）＋ `list.for_each` ＋ `tts.speak` で explicit に組み立てる。ドライバの責務を「1 発話＝1 合成＝1 再生」に狭める δ-4b で確立した explicit-iteration 方針を TTS にも適用。
> - **`save_path` の共通処理**: `screenshot.capture` と同じ `{T}` テンプレ展開ヘルパ `resolve_save_path` を TTS でも共有。WAV bytes を握れるドライバ（OS TTS Windows / VOICEVOX 系 / CoeiroInk）は `maybe_save_wav(&wav, &req.save_path)` で一発保存。Bouyomichan や非 Windows の OS TTS のように bytes を持たないドライバは WARN ログを出して無視（契約上問題なし）。
> - **VoicePeak（δ-4c.1 ✅）**: VoicePeak の CLI `voicepeak.exe -s "text" -o out.wav -n <narrator> --emotion "..." --speed 100 --pitch 0` を `tokio::process::Command` で spawn し、生成 WAV を読んで `audio_sink` へ append する `drivers/voicepeak.rs` を追加。タイムアウトは 60 秒、narrator 名は `voice` 入力（または `extra.narrator` で上書き）、emotion CSV は `extra.emotion`。グローバル既定 exe は `conf.toml` の `[voicepeak].path`（空なら OS 既定）を起動時に解決し、`tts.speak` が `endpoint` 空の VoicePeak 発話で補填する。ドライバ側の解決は **`req.endpoint`（非空）を最優先**、空なら非推奨の `extra.executable`（互換・1 回 warn）、それも空ならバイナリ名 `voicepeak`（PATH）。正規化マッピングは `speed: 1.0 → --speed 100 (clamp 50..200)`、`pitch: 0.0 → --pitch 0 (clamp -300..300)`。VoicePeak CLI は音量オプションを持たないため `volume != 1.0` は WARN を 1 行出して無視する。一時 WAV は `std::env::temp_dir()` 配下に `vac-voicepeak-{timestamp}-{pid}.wav` で生成し、再生・保存後に必ず削除。テストは (a) speed/pitch マッピング、(b) 最小 / フル引数の CLI 組み立て、(c) `extra.narrator` / `*_raw` 優先、(d) exe 解決順序、(e) 到達不能 exe → `TtsError::Io | Synthesis` への畳み込み、の 6 ケース。
> - **テスト戦略**: δ-4b 同様 HTTP/TCP モックは使わず、(a) no-fire noop、(b) 未知 engine 名の `on_error`、(c) 空テキストの `on_error`、(d) 到達不能エンドポイントの非致命エラー化、(e) 各ドライバの `voice` パース・スケール関数・TCP パケット生成の単体テスト、の 5 層で契約を確認する。総計 25 ケースが `flowgraph::{tts,nodes::tts}` 配下で passing。

> **δ-4d 実装ノート**（2026-04-17）: V1 `TwitchOut`（単一 processor に「token validate / user_id 解決 / chat 送信 / rate limit / strip / clip / 401 リフレッシュ再送」の 7 責務を詰め込んだモノリス）を **4 つの小さな Flowgraph ノードへ分解**した。理由は δ-4c の TTS 統合とは **逆方向の最適化**で、Twitch は token 管理や broadcaster_id 解決といった**前処理の段階**がユーザごとに大きく異なり、graph で可視化・差替え可能にしたい要求が強いため。同じ graph 上で `ban` / `timeout` / `delete_message` / `set_category` 等の Helix action を後から追加する際、`access_token` / `client_id` / `broadcaster_id` の解決ロジックを使い回せる形にしておきたい（δ-4d.1 以降で Helix action ノードを個別に追加する計画）。
>
> - **新ノード（6 種）**:
>   - `flowgraph.twitch.validate_token` (Effectful): `GET https://id.twitch.tv/oauth2/validate`。token 所有者（bot 本体）の `user_id` / `login` / `client_id` を取得。`sender_user_id` の解決に使う。
>   - `flowgraph.twitch.user_id_by_login` (Effectful): Helix `GET /users?login=...`。`broadcaster_login` から `broadcaster_id` を解決。
>   - `flowgraph.twitch.chat_send` (Effectful): Helix `POST /chat/messages`。実際のチャット送信。`max_chars` / `strip_substrings` は optional 入力で V1 互換。`endpoint` 入力で send-to-API base URL を差替え可能（自前プロキシ / モック用）。
>   - `flowgraph.twitch.ban` (Effectful): Helix `POST /moderation/bans`（`duration` 未指定＝永久 ban）。スコープ `moderator:manage:banned_users` 必須。`reason` は 500 文字で自動クリップ、`end_time` を出力（永久 ban は null/空）。
>   - `flowgraph.twitch.timeout` (Effectful): 同 Helix `POST /moderation/bans`（`duration_secs` 指定＝時間制限 ban）。範囲は **1..=1209600 秒（2 週間）**、範囲外は `on_error` に落ちる。API 本体は `ban` と完全共有（内部ヘルパ `execute_moderation_ban(inputs, Option<duration>)` でラップ）。
>   - `flowgraph.util.rate_limit` (Stateful): 汎用トークンバケット（N 回 / X ms）。V1 の `recent_sent: VecDeque<Instant>` 相当の状態を engine 管理下の state slot に格納。Twitch 以外（Discord webhook / 任意 HTTP 連打防止）でも使える。
> - **なぜ ban / timeout を 2 ノードに分けたか**: Helix は `duration` の有無で両者を区別する同一エンドポイントなので、`duration_secs` を optional 入力にした統合ノード案もあった。が、GUI（δ-6）から見たとき「何をするノードか」はラベルで瞬時に判別したい性質のアクションなので、**ラベル駆動**で 2 ノードに分離した（TTS と逆向きの判断）。ユーザはノードを置いた段階で意図が確定するので、`duration_secs` の意図的な未接続による「気づかずに永久 ban」事故が起きにくい。内部実装は完全共有なので保守コストは最小。
> - **廃止した V1 内部動作と graph-level での代替**:
>   - 起動時の access_token 取得 → ユーザが `state.latch` に token を入れて、上流の trigger で初期 populate する。
>   - 起動時の broadcaster_id / sender_user_id 解決 → `ingress.web_input` 等の起動トリガ一発 + `validate_token` / `user_id_by_login` + `state.latch` で組む（1 回だけ走る HTTP 解決を graph 上で明示化）。
>   - 401 発生時の refresh + 1 回再送 → `chat_send.on_error` から refresh サブグラフへ分岐し、token を `state.latch` に更新して `chat_send` を再 trigger する明示的リトライループ。「黙って裏でリカバリ」する V1 の暗黙動作を graph で可視化できるようになった（エラーハンドリング戦略は persona ごとにユーザが変えたいことが多い）。
>   - 自 bot login の ingress ignore list 自動注入 → ingress 側で `compare.eq(source_actor, my_login)` + `flow.gate` でエコー防止。V1 では可視化されていなかった「echo 判定」が明示的に図示される。
>   - 30s 窓の rate 制御 → `util.rate_limit` を `chat_send` の上流（exec 経路）に挿入。`on_deny` 経路をユーザが使って drop ログや UI 通知に流せる（V1 は warn ログ 1 行で黙って捨てていた）。
> - **`chat_send` の 3 分岐 exec 出力**: `on_success` / `on_error` / `on_skipped` を提供。`on_skipped` は「strip/clip の結果 text が空になった」ケース用で、V1 は `CompletedAnd::Next` + debug ログで無言に続行していたものを graph 上で明示的に観測できるようにした。
> - **`text` 入力の加工順序**: V1 と同じく `trim → strip_substrings → trim → clip_chars(max_chars)` の順。`max_chars <= 0` は no-op（`max(1)` で正規化）、非 ASCII の 4-byte 文字はグラフェム境界未考慮の簡易実装（V1 互換）。
> - **`endpoint` 入力による差替え**: 3 ノードとも Helix / validate URL のベースを optional 入力 `endpoint` で上書き可能。これは (a) 自前プロキシ / エンタープライズ中継の利用、(b) **テストでの localhost:1 向けたロールバック**（HTTP モック不要で `on_error` 経路を検証できる）、の 2 用途を狙っている。
> - **`util.rate_limit` の設計**: max_count と window_ms の 2 入力で N/window のトークンバケットを実装。exec 発火で窓内に空きがあれば `on_allow` + 残量 `remaining`、埋まっていれば `on_deny` + `remaining=0`。exec 未発火の pull（純粋 data 要求）では残量だけを返す。汎用な `StatefulNode` として他ノードと同じ合成則に乗るので、graph 内に何個でも置ける（キャラ別レートリミット、API 別レートリミット等）。
> - **HTTP client の使い回し**: 各 EffectfulNode は `reqwest::Client::builder().timeout(30s).build()` を都度生成している。コネクションプールの再利用は δ-4 終了後の `ExecCtx::http_client` 拡張で一括対応予定（現状は latency インパクト小なので未着手）。
> - **統合テスト**: graph ビルド → `run_forever_with_bus` で外部 trigger 駆動 → trace 検査、のパターンを 2 種:
>   (1) `ingress.twitch → rate_limit(max=2) → log_{ok,ng}` の 3 連打で `ALLOW×2 / DENY×1` を観測（レート制御の graph-level 動作検証）。
>   (2) `ingress.twitch → chat_send(endpoint=127.0.0.1:1) → log_err` で connection refused 経由の `on_error` 分岐到達を観測（非致命エラー規約の end-to-end 検証）。
> - **テスト戦略**: δ-4b/4c 同様 HTTP モックは使わず、(a) no-fire noop、(b) 入力バリデーションによる `on_error`（user_id / broadcaster_id / moderator_id の空チェック）、(c) strip 後空テキストの `on_skipped`、(d) 到達不能 endpoint の非致命エラー化、(e) `clip_chars` / `strip_substrings` の単体テスト、(f) rate_limit の allow/deny/窓リフィル、(g) `timeout.duration_secs` の範囲チェック（0 / 2週間超で `on_error`、`get_required_int` 未接続で `MissingRequiredInput`）、(h) graph 統合の 2 シナリオ、の 8 層で契約を確認する。総計 22 ケースが `flowgraph::nodes::{twitch,rate_limit}` 配下で passing。lib 全体では **267 tests / 0 failures**、clippy `--lib --no-deps` で新規 warning ゼロ。
> - **δ-5（Loader）への引き継ぎ**: 「V1 `[[processors]] feature = \"twitch-out\"` → 4 ノードの展開テンプレート」を migrate CLI (δ-8) が自動生成する想定。ユーザが手書きする最短 graph は `validate_token + user_id_by_login + state.latch×3 + rate_limit + chat_send` の 6 ノード。テンプレートファイルとして `flowgraph/examples/twitch-out.v2.toml` を δ-8 で同梱する。


δ-1 の `FlowgraphProgram::execute()` は「初期ソースから 1 回評価して完走」する 1-shot 型。δ-3 で **常駐 event loop 型 `run_forever()`** に昇格させる。

- **Trigger Source 抽象**: `async fn next_trigger(&self) -> Triggered { node_id, fired_exec, data_patch }` を実装する entity。
  - **外部 trigger**: Ingress 系 `EffectfulNode`（WebInput / Voice / Twitch）が該当。外部イベント（HTTP POST / 音声区切り / IRC メッセージ）で `next_trigger` が目覚める。
  - **内部 trigger**: `util.delay` は自分自身を trigger source 化する（`StatefulNode` かつ trigger source）。exec_in 発火で `(data_snapshot, sleep_duration)` をキャプチャして即 return し、`tokio::spawn` で sleep 後に engine の trigger キューに投入する仕組み。**blocking せず**、他の exec チェーンを止めない。
- **event loop**: `tokio::select!` で全 trigger source を多重待機。いずれかが発火すると engine は `generation += 1` し、対応ノードから exec chain を 1 パス回す。
- **外部 trigger と 1-shot 実行の両立**: テストや migrate CLI は「外部 trigger を一切使わずに初期発火だけで 1 回評価して終わる」1-shot モードを残す。`execute()` は 1-shot、`run_forever()` は event loop、という 2 API 併存。

δ-1 v2 は `execute()` の 1-shot API のみ提供し、δ-3 で `run_forever()` を追加する形で拡張する（既存 API 破壊なし）。

---

## 6. Flowgraph モジュールシステム

### 6.1 フォルダレイアウト

```text
<project_root>/
├── conf.toml                       # アプリ設定（AI persona / run_with / obs など）
├── flowgraph/                      # Flowgraph ルート（固定名、conf 側で変更可）
│   ├── main.flowgraph.toml         # ルートの既定ファイル
│   ├── utils.flowgraph.toml
│   ├── ingress/
│   │   ├── main.flowgraph.toml
│   │   └── twitch.flowgraph.toml
│   ├── tts/
│   │   ├── main.flowgraph.toml
│   │   └── jp_routing.flowgraph.toml
│   └── experiments/
│       └── wip.flowgraph.toml.disabled   # 自動的に skip
```

### 6.2 ロード規則

- `flowgraph/` 以下を再帰的に走査し、`*.flowgraph.toml` 全件をロード。
- `*.flowgraph.toml.disabled` は無視（サフィックスマッチのみ、prefix `.` でも可だが spec としては `.disabled` を推奨）。
- 隠しフォルダ（`.` 始まり）と `_` 始まりフォルダは走査スキップ（実験用・IDE 専用フォルダ退避）。
- 拡張子が `.flowgraph.toml` 以外のファイル（例: `README.md`）は無視。
- 同一 fq name のノードが複数ファイルに存在したらエラー（起動停止 or GUI 赤表示、どちらかは loader の strict / lenient モード切替で決定）。

### 6.3 main.flowgraph.toml 規約

- `X/main.flowgraph.toml` は `X/` フォルダの「既定ターゲット」。
- 他ファイルからは `X/` だけで `X/main` を参照可。例: `tts::node_xxx` は `tts/main::node_xxx` と等価。
- ルート `flowgraph/main.flowgraph.toml` も同様。トップレベルの fq name は `main::node_id` が等価。
- **main が必須ではない**: `X/` に `main.flowgraph.toml` が無くても、`X/foo.flowgraph.toml` だけで運用可。ただしその場合 `X::node_id` のような短縮参照が出てくると loader は **warning** を出す（δ-0 合意）。GUI Flowgraph エディタは該当 warning に対して「空の `main.flowgraph.toml` を作成して解消」ボタンを 1 クリックで提示する。ランタイムは warning のまま当該エッジ/ノードを skip、起動は継続。

### 6.4 fq name 構文

- 文法: `path::node_id`
  - `path`: `/` 区切りのフォルダ/ファイル名列（拡張子なし、`.flowgraph` も削除）
  - `node_id`: そのファイル内で unique な識別子
- 相対パスの解決:
  - `node_id` — 同一ファイル内のノード参照
  - `./file::node_id` or `file::node_id` — 同一フォルダ内ファイル
  - `subfolder/file::node_id` or `subfolder::node_id`（main 規約）
  - `../sibling/file::node_id` — 親経由
  - `/tts/main::node_id` — ルート絶対パス（先頭 `/` で `flowgraph/` からの絶対）
- 保存時の正規化: loader は解決後 **絶対 fq name** に正規化してメモリ保持（ファイルへ書き戻す際は相対形で保存してファイル可搬性を守る）。

### 6.5 ファイル内部の命名

- `node.id`: そのファイル内で unique な snake_case。文字 `[a-z0-9_]+` 推奨。
- ファイル名: `[a-z0-9_]+\.flowgraph\.toml`（kebab-case 禁止。fq name 解析と TOML section key の整合のため）。
- フォルダ名: `[a-z0-9_]+`。

---

## 7. TOML フォーマット仕様

### 7.1 ファイル選定の根拠と却下理由

Phase δ では Flowgraph ファイルフォーマットに **TOML** を採用する。代替候補と比較した上での決定:

| 候補 | 採否 | 理由 |
| ---- | ---- | ---- |
| **TOML** | **採用** | 既存 `toml_edit` による format-preserving 編集が γ 期で複数箇所に実装済み。`[[nodes]]` / `[[edges]]` の配列テーブルが fragment 切り貼りと相性良好。literal string `'...'` / multi-line `'''...'''` で regex・Windows パスをエスケープ無し記述可能。プロジェクト全体（conf.toml / Bruno / dictionary）との文化統一。 |
| JSONC / JSON5 | 却下 | format-preserving 編集と regex エスケープを自前実装する負荷が高い。GUI 結合は楽だが、手編集時の摩擦が TOML より大きい。 |
| KDL | 却下 | 構造は Flowgraph に最も自然だが、Rust (`kdl-rs`) / TS (`kdljs`) 双方で format-preserving ドキュメント編集の成熟度が不足。新フォーマットの学習コスト。 |
| YAML | 却下 | インデント事故、`no` → `false` の quoting 事故などユーザー手編集で地雷が多い。 |
| HCL | 却下 | 構造的相性は良いが、Rust / TS ともに format-preserving editor が事実上存在しない。オーバーキル。 |

### 7.2 ファイル全体のスキーマ

```toml
# 各 .flowgraph.toml はトップレベルで 1 つのファイルを表現する

[meta]
title = "日本語 TTS ルーティング"           # 任意
description = "Twitch 発言を JP TTS に流す" # 任意
tags = ["tts", "jp"]                        # 任意: GUI 検索用

# --- ノード定義 ---
[[nodes]]
id = "literal_voice"                    # このファイル内 unique
feature = "flowgraph.literal.string"    # NodeSpec.feature
position = [100, 200]                   # GUI のみ使用。[x, y]
# properties はノード仕様に従う
properties.value = "伊織ゆう"

[[nodes]]
id = "tts_jp"
feature = "flowgraph.tts.voicevox"
position = [400, 200]
properties.host = "127.0.0.1:50021"
properties.speaker_id = 3

# --- エッジ定義 ---
[[edges]]
from = "literal_voice:value"
to = "tts_jp:voice_name"

# 他ファイルのノードからの入力（fq name + 相対パス）
[[edges]]
from = "../ingress/twitch::main:content"
to = "tts_jp:text"

[[edges]]
from = "../ingress/twitch::main:exec_out"
to = "tts_jp:exec_in"
```

### 7.3 ポート参照 文字列文法

- `from` / `to` の文字列: `<node_ref>:<port_name>`
  - `node_ref` 部分は fq name 構文（§6.4）に従う
  - `port_name` は `[a-z0-9_]+`
- node_ref と port_name の区切りは **単一コロン `:`**。fq name 内部の `::` と混同しない（port 側は単一、fq は二重）。
- 例:
  - `local_node:out` — 同一ファイル内
  - `tts::main:exec_out` — 兄弟フォルダ tts の main ファイルの exec_out（main 規約）
  - `../ingress/twitch::main:content` — 相対パス解決
  - `/sink/bos::final:text` — ルート絶対パス

### 7.4 properties の値表現

- プリミティブ（Bool / Int / Float / String）は TOML 標準型をそのまま使う。
- `List<T>`: TOML 配列。`properties.tags = ["a", "b", "c"]`
- `Map<String, T>`: TOML テーブル。ただしノード内の他キーと衝突しないよう `properties.headers` のような dotted key を使う。
- `Json` 型のプロパティ: **2 通り許容**
  - (a) inline table + 配列でできる範囲: `properties.config = { host = "...", port = 8080 }`
  - (b) 深いネストやユニオン型を含む任意 JSON: literal multi-line 文字列として埋め込み + `raw_` プレフィクス規約。

    ```toml
    properties.raw_openai_tools_json = '''
    [
      {"type":"function","function":{"name":"lookup","parameters":{...}}}
    ]
    '''
    ```

  - ランタイムで `raw_xxx_json` suffix を検出したノードは `serde_json::from_str` でパースして使う。GUI は multi-line JSON エディタを提供する。

### 7.5 深いネストガイドライン

- 2 階層まで: inline table or dotted key 推奨。
  - `properties.auth.token = "xxx"`
  - `properties.auth = { token = "xxx", expires_at = 1234 }`
- 3 階層以上 or 配列を含む: inline table は可読性が崩壊するので次のいずれか:
  - ノード自体を分解して子ノードに切り出す（設計原則として推奨）
  - `raw_xxx_json` literal multi-line string に逃がす（§7.4 b）

### 7.6 regex / パス / URL のエスケープ

- regex パターンは literal string を使う。

  ```toml
  [[nodes]]
  id = "regex_number"
  feature = "flowgraph.regex.match"
  properties.pattern = '\d+(\.\d+)?'
  ```

- Windows パスも同様。

  ```toml
  properties.output_dir = 'C:\Users\me\logs'
  ```

- 複数行にまたがる場合は `'''...'''`。

### 7.7 コメント

- TOML 標準の `#` 行コメント。ブロックコメントは無い。
- GUI による書き戻しで**コメントは保持する**（`toml_edit` の挙動に従う）。

---

## 8. Loader アルゴリズム

### 8.1 入力

- `flowgraph_root: PathBuf`（通常は `<project>/flowgraph/`、`conf.toml` で変更可能）

### 8.2 ステージ

1. **ファイル列挙**: 再帰ウォーク、`.flowgraph.toml` フィルタ、`.disabled` / 隠しフォルダ / `_` 始まり skip。
2. **パース**: 各ファイルを `toml_edit::DocumentMut` で読み込み、`FlowgraphFile { meta, nodes, edges }` に deserialize。パースエラーはファイル単位で収集して続行。
3. **ファイルパス → fq path 変換**: `flowgraph/tts/jp_routing.flowgraph.toml` → `tts/jp_routing`。
4. **ノード登録**: `(fq_path, node.id)` → 絶対 fq name `tts/jp_routing::node_id` に展開、グローバル `NodeRegistry` へ登録。重複は error diagnostic。
5. **feature → NodeSpec 解決**: 各ノードの `feature` をノードカタログと突合。未知 feature はエラー（該当ノード skip、依存エッジも skip）。
6. **プロパティ検証**: `PropertySpec` と突合して型・required・validator チェック。
7. **エッジ解決**:
   - `from` / `to` の文字列を fq name + port name にパース
   - 相対 fq name は現在のファイルパスを起点に絶対化
   - `NodeRegistry` から対象ノードを引く（未解決はエラー）
   - ポート存在確認
   - 型整合・exec/data 整合・multi 制約チェック
8. **循環検出**: data DAG / exec DAG それぞれに Kahn's algorithm で検査。
9. **必須入力チェック**: 必須 input が未接続かつ default なしのノードは **error**（起動停止の原因になる。δ-0 合意）。
10. **グラフ構築完了**: `FlowgraphProgram { nodes, edges, topo_order }` を state にセット。

### 8.3 診断情報（diagnostics）

- 各診断は `{ severity, file, line?, column?, message, code }` 形式。
- severity: `error` / `warning` / `info`。
- GUI にそのまま配信して該当ファイル/ノードを赤/黄枠で表示。
- Control API: `GET /api/v1/control/flowgraph/diagnostics` で JSON 取得。

### 8.4 書き戻し（保存）

- ファイル単位で `toml_edit::DocumentMut` を保持し、変更のあったファイルのみ diff minimal で書き戻し。
- コメント・空行・キー順は保持する（既存 `conf_edit.rs` パターンに従う）。
- 保存は atomic（`.tmp` 作成 → rename）、失敗時は `.bak` に退避。

### 8.5 ホットリロード

- ファイルウォッチャーで `flowgraph/` を監視。変更検出時に部分再ロード（差分のみ再パース・再検証）→ `EventBus::FlowgraphReloaded` 配信。
- 実装は δ-5 Phase で検討。δ-0 では必須仕様には含めず optional として記載。
- δ-6d 時点の実装方針（2026-04-17）: **`notify` crate による自動ウォッチャーは採用しない**。代わりに
  - `POST/PUT/DELETE /flowgraph/file*`（GUI 由来の write）では **書き込み直後に同期リロード + `FlowgraphReloaded` WS push**。
  - 外部エディタで `.flowgraph.toml` を直接編集したケースは、GUI の「Reload」ボタン相当の **`POST /flowgraph/reload`** を明示的に叩くと同じリロード + WS push が走る。
  - これで GUI 側の UX は「編集したら即反映される」ことが保証される（どちらのルートでも `FlowgraphReloaded` が飛ぶ）。`notify` 依存の追加・OS 依存のデバウンス調整・エディタの atomic rename 対応などの運用コストを δ-7 以降へ先送りした。

`ControlEvent::FlowgraphReloaded` のペイロードは bandwidth 節約のためサマリのみ: `{ root_dir, ok, error_count, warning_count, node_count }`。GUI は本イベントをトリガに `GET /flowgraph/diagnostics` / `GET /flowgraph/tree` を pull して画面を更新する。

### 8.6 δ-5 実装メモ（2026-04-17 完了）

> - **モジュール構成**: `src/flowgraph/registry.rs`（ノードカタログ）+ `src/flowgraph/loader/{mod,file,dir,reference,diagnostic}.rs`（パース / 解決 / ビルド）の 2 階層。`FlowgraphProgram` 本体は δ-3 までで完成済みなので、δ-5 は純粋に **TOML → Builder 呼び出しのブリッジ層** として実装した。
> - **`NodeRegistry`**: `feature` 文字列（例: `"flowgraph.literal.string"`）をキーに、`Arc<dyn PureNode|StatefulNode|EffectfulNode>` を singleton 保持するマップ。`LazyLock` による static グローバルで起動時 1 回初期化。δ-5 時点で **61 feature** を登録（`flow.sequence` は出力数が instance 依存のため対象外、δ-6 以降で properties 駆動化予定）。`make_impl(feature)` は Stateful ノードについても内部で `init_state()` を再実行するので、同じ feature を複数インスタンス化しても state slot は独立。
> - **`FlowgraphFile` スキーマ**: δ-5 時点では `[meta] / [[nodes]] / [[edges]]` の 3 セクションのみとした。**Phase λ** 以降、後方互換のもとで **`[[enums]]`（任意・ユーザ定義閉集合）** と **`[meta]` の追加任意フィールド**（`author` / `name` / `version` / `license` / `repos` / `library_uses` 等）を許容する。未指定は空扱い。設計正本: [`phase-lambda-flowgraph-enum-and-library.md`](phase-lambda-flowgraph-enum-and-library.md) §4。`include` ディレクティブは**採用せず**（フォルダ走査で十分、明示的 include は fq path 体系と二重の参照経路になって混乱する）。`nodes.id` / `nodes.feature` は必須、`position` / `properties` は optional。`edges` は `from` / `to` の 2 文字列のみで `kind = "exec"` のようなヒントは取らず、**ポート spec の `is_exec` フラグで自動判定**する（TOML 側での重複指定をなくすため）。
> - **ポート参照文字列**: `node:port` / `path::node:port` を `parse_port_ref` で decomposition。右端から「`::` でない単独 `:`」を探す lexer で `__trigger__` のような underscore-heavy port 名とも衝突しない。fq path 側の `..` / `./` / 先頭 `/` を `resolve_fq_ref` で正規化し、`../../` で root 外に出るケースは error として早期検出。
> - **main 規約解決**: `X::id` の参照は、既知 fq に `X` 単体が無く `X/main` がある場合のみ後者に書き換える fallback 実装。「`X.flowgraph.toml` と `X/main.flowgraph.toml` が両方ある」状況では直接ヒットを優先し、main fallback は発動しない（spec §6.3 の strict 解釈）。
> - **プロパティ解決**: `toml::Table` → `InputMap`。未宣言プロパティは **warning**（typo 検出用、ランタイムには影響しない）、必須未指定 / 型不一致は **error**。デフォルト値は `PropertySpec.default` から `SocketValueRepr::to_socket_value` で復元して自動充当。
> - **Diagnostic 設計**: `{ severity, code: DiagnosticCode, message, file, node, hint }` の serializable 構造体。`code` は 12 種類のケバブケース enum（`unknown-feature` / `invalid-port-ref` / `property-type-mismatch` / ...）で、GUI 側の「Fix it」分岐を文字列比較でなく型で書けるようにした。1 件のエラーで即 abort せず、**可能な限り複数診断を収集してから失敗する**（ユーザが 1 回のビルドで複数問題を俯瞰できるようにする）。line / column はこのフェーズでは未対応（`toml` crate が span を出さないため）、δ-6 で `toml_edit` の span 付きパースに切替予定。
> - **エントリポイント**: `load_file(path)` は単一ファイル用（テスト・small-scope 向け、fq prefix は `"main"` 固定）、`load_flowgraph_dir(root)` が本番想定（`flowgraph/` ルート配下を再帰 walk → 全ファイル統合）。`.flowgraph.toml.disabled` / `.` 始まり / `_` 始まりフォルダはエントリポイント共通で skip。
> - **テスト戦略**: 3 層構成で計 36 ケース。
>   (a) `reference.rs` 単体パーサ: 14 ケース（local / fq / 相対 / 絶対 / main 規約 hit / main 規約 no-fallback / root 外参照 error / 空文字 / internal port `__xxx__` / 数字 underscore 混在 node_id）。
>   (b) `file.rs` 単一ファイル loader: 9 ケース（happy path / 未登録 feature / 未知ポート / 未解決ノード / id 重複 / プロパティ型不一致 / 未知プロパティ warning / edge port-ref 不正 / TOML minimal parse）。
>   (c) `dir.rs` 統合 loader: 8 ケース（walker フィルタ / fq path 変換 / cross-file edge / main 規約 / 相対パス edge / 空 root / 存在しない root error / **リポジトリ同梱の `flowgraph.example/` を実ファイル経由でロードする回帰テスト**）。
>   また `registry.rs` に 5 ケース（デフォルト registry の全 feature 含有チェック / `make_impl` が Pure/Stateful/Effectful を正しく返す / unknown feature が None）。lib 全体では **303 tests / 0 failures**、clippy `--lib --no-deps` で新規 warning ゼロ。
> - **`flowgraph.example/` 同梱**: repo 直下に minimal (`main.flowgraph.toml`: literal → log) と multi-file (`chat-echo/main.flowgraph.toml` + `chat-echo/tts.flowgraph.toml`: twitch ingress → cross-file tts.speak) の 2 サンプルを配置。CI でこれをそのまま loader が取り込めることを保証することで「サンプルが古くなる」問題を回避する。δ-8 の migrate CLI で生成する v2 テンプレートのリファレンスにもなる。
> - **δ-6 への引き継ぎ**: `LoadReport { program, diagnostics, node_meta }` の `node_meta: HashMap<fq_name, { feature, file, position }>` が GUI のノード描画と「このノードはどのファイルから来たか」表示に直結する。`Diagnostic` はそのまま `GET /api/v1/control/flowgraph/diagnostics` の JSON body として使える（`Severity` / `DiagnosticCode` 共に `serde_as` で kebab-case serialize）。ホットリロードは §8.5 に従い δ-6 で file watcher 経由で `load_flowgraph_dir` を再実行 → diff → `FlowgraphProgram` スワップで対応予定。

### 8.7 δ-6 実装メモ（2026-04-17 完了）

> - **全体分解**: δ-6a（Conf + State 保持）→ δ-6b（Control API GET）→ δ-6c（Control API write）→ δ-6d（WS push + `POST /flowgraph/reload`）→ δ-6e（GUI Flowgraph タブ）→ δ-6f（spec / テスト仕上げ）の 6 サブフェーズで進行。ひとつの PR では荷が重いので分割が正解だった。
> - **State に何を積むか**: `FlowgraphProgram` は `Box<dyn Any + Send>`（state slot）を含むため `!Sync`。これを `tokio::sync::RwLock<State>` 越しに持つと State 全体が `!Sync` になり `SharedState = Arc<RwLock<State>>` の要件を満たさない。解決策として State には **`FlowgraphRuntime = { root_dir, ok, diagnostics, node_meta }` のメタデータだけ** を保持し、`FlowgraphProgram` 本体は δ-8 で起動する worker task（`Send` で十分）にムーブする設計に落とした。GUI は metadata だけ見えれば 100% 実装可能なので問題にならない。
> - **Control API ルート**: `GET /flowgraph/{node-catalog,tree,file/{fq},diagnostics}` / `POST /flowgraph/file` / `PUT,DELETE /flowgraph/file/{fq}` / `POST /flowgraph/file/{fq}/open-external` / `POST /flowgraph/reload`。`{fq:.*}` ワイルドカードの登録順序（`open-external` を `GET /file/{fq}` より前に）に注意。パス検証は **①`..` / 絶対パス / `:` を含む fq を早期拒絶 → ②`fq_to_file_path` で `<root>/<fq>.flowgraph.toml` に正規化 → ③`ensure_within_root` で canonicalize 後 root 配下確認** の 3 段。書き込み系は毎回 `.bak` を作成し、誤 PUT / 誤 DELETE の復旧余地を残した。
> - **ホットリロード（spec §8.5 と整合）**: `notify` crate 依存を避け、**write 系 API が reload を自分で発火 + `FlowgraphReloaded` WS push** するポリシー。外部エディタ編集後の「Reload」は GUI ボタン → `POST /flowgraph/reload` で同じコードパスを通る。GUI はどちらの経路でも最終的に `ControlEvent::FlowgraphReloaded` 1 件で tree / diagnostics を pull し直す（payload は summary のみで bandwidth 節約）。`notify` ベースの自動ウォッチャーは δ-7 以降に後回し。
> - **GUI 構成（δ-6e）**: Svelte 5 + `@xyflow/svelte`（Svelte Flow）+ Skeleton UI。Flowgraph タブを Live / Setup / Pipeline / Logs / Tools の間に追加し、5 コンポーネントで構成:
>   - `FlowgraphTree.svelte`（左）: `GET /flowgraph/tree` の flat list を `/` 区切りで階層化、選択・新規作成（prompt で fq 入力）・削除（.bak 退避）・診断バッジを兼任。
>   - `FlowgraphCanvas.svelte` + `FlowgraphNodeCard.svelte`（中）: Svelte Flow にカスタム `nodeTypes`。ノードカードは `inputs` / `outputs` を左右に並べ、`is_exec` ポートは橙色矢印、data ポートは青丸で描き分け。接続時は型チェック（`is_exec` 一致 + `ty` 一致 or `json` 総称受け入れ）をフロント側で実施し、不正接続は no-op で捨てる。`__foo__` の内部ポートはカード描画から除外（spec §5.1 準拠）。
>   - `FlowgraphPalette.svelte`（右上）: `GET /flowgraph/node-catalog` をカテゴリ別に並べ、検索ボックスで title / feature / description をインクリメンタル絞り込み。クリックで現在ファイルに新規ノードを追加（既存ノードの重心 + ランダム offset で配置、id は feature の末尾セグメントから自動生成）。
>   - `FlowgraphPropertyEditor.svelte`（右下）: 選択ノードの properties を型別に編集。bool=checkbox / int,float=number input / string=text input / json,list,map=JSON textarea（parse 成功時のみ store 反映）。未設定の optional プロパティは `default` 表示 + Set ボタン、仕様外プロパティは警告バッジ + Remove ボタンで typo 検出が視覚化される。
>   - `FlowgraphDiagnostics.svelte`（下）: `GET /flowgraph/diagnostics` をテーブル表示、行クリックで該当ファイル / ノードへジャンプ。
> - **保存戦略**: Draft 層（`flowgraphStore.draftNodes / draftEdges`）で node 位置とプロパティを GUI 側に持ち、Save 時に **自前の簡易 TOML シリアライザ**（`serializeFlowgraph`）で `.flowgraph.toml` を丸ごと書き出す方針を取った。`toml_edit` 相当のフォーマット保持は JS で再実装すると肥大化するので、「GUI 編集は format-lossy でよい、コメントを残したいファイルは Open External」に UX を割り切った（spec §7.4 のコメント保持要件は外部エディタ経由でカバーする）。
> - **WS 統合**: `events.svelte.ts` の `KNOWN_KIND_LIST` と `types.ts` の `ControlEvent` discriminated union に `flowgraph_reloaded` を追加し、`EventStream.svelte` / `ToastBridge.svelte` の exhaustive switch の漏れが compile-time に検出される設計を維持。`FlowgraphTab.svelte` の `onMount` で `flowgraphStore.attachWsSubscriber()` を呼び、`flowgraph_reloaded` 受信時に tree / diagnostics / 現在ファイルを並行 pull。
> - **型整合**: Rust 側 `NodeSpec / PortSpec / PropertySpec / Diagnostic / LoadedNodeMeta / FlowgraphFile` に対応する TS 型を `types.ts` に 1:1 で追加。`SocketType` は Rust 側が文字列 serialize するので TS 側も `FlowgraphSocketType = string` として素通しし、UI は `list<` / `map<` プレフィックスと primitive name のマッチングで振り分ける。
> - **検証**: `svelte-check` 0 errors / `vite build` 成功 / 新規コードに対して eslint 警告ゼロ（残る 5 件は本フェーズで触れていない既存ファイル）。Rust 側は `cargo build --lib` 通過・既存 306 test 全通し・clippy `--lib --no-deps` で新規 warning なし。
> - **スコープ外（δ-7 以降）**: auto-layout、multi-select 編集、copy/paste、undo/redo、graph のリアル実行（`run_forever`）、`notify` 連動の自動ファイル監視。

### 8.8 δ-7 実装メモ（2026-04-17 完了）

> - **全体分解**: δ-7a（Fragment copy / serialize）→ δ-7b（paste / remap / 衝突解消）→ δ-7c（Control API `fragment/copy` + `fragment/paste`）→ δ-7d（ZIP export/import + manifest + 安全ルール）→ δ-7e（GUI: copy/paste ボタン + share dialog）→ δ-7f（spec 更新 + 最終テスト）の 6 サブフェーズ。
> - **モジュール境界**: `src/flowgraph/fragment/{mod,paste,zip_codec}.rs` の 3 本で自己完結。`mod.rs` が copy / serialize / parse を、`paste.rs` が file/node レベルの書き戻し（衝突戦略・remap・offset）を、`zip_codec.rs` が ZIP 層（manifest + companion）を担当する分離。依存の向きは `zip_codec → paste → mod` の一直線で循環しない。
> - **Fragment schema（§9.2）**: spec 初稿の `kind / original / reason` 3 フィールド必須のまま、paste 時の正確な remap に必要な `in_file / edge_from / edge_to / external_side` を **v1 後方互換 optional として追加**。古い v1 fragment（これら欠損）も読み込み可能で、その場合 dangling は unresolved のまま残す挙動。
> - **Paste エンジン（§9.4）**: `OnConflict`（node id 衝突: `suffix`/`skip`/`overwrite`）と `OnConflictFile`（ファイル衝突: `rename`/`skip`/`overwrite`/`merge`）の 2 軸で衝突解消、`position_offset: [x, y]` でカーソル位置補正、`remap: { original -> new | null }` で dangling を reconnect（`null` 明示で drop、未指定は unresolved に残す）。`merge` は既存ファイルに fragment の node/edge を追記、衝突 node は `OnConflict` でさらに裁く。paste 完了時は `.bak` を残すので、誤 paste もファイル単位で復旧できる。
> - **ZIP codec（§9.3）**: `manifest.flowgraph.toml`（header + danglings）+ `flowgraph/**/*.flowgraph.toml` + companion（non-flowgraph files）の 3 要素構成。安全ルール（`..` / 絶対パス / Windows ドライブ / null byte / Unix symlink / 危険拡張子）は **エントリ全走査時に即拒否**で 1 バイトも書かない。dry_run=true は paste を走らせず、preview（entries / conflicts / danglings / target_prefix）だけ返す。
> - **Control API（§9.4）**: spec 初稿の `GET /export/zip` は **POST に変更**（targets を query で送れないため）。`import/zip` は `actix-multipart` を避けて **raw ZIP body（`web::Bytes`）+ query params** にした。body limit は `configure()` で `PayloadConfig::new(64 MiB)` を app_data 経由で scope 全体に効かせる（他 route は JSON 数 KiB なので影響なし）。paste / import は完了時に `reload_runtime` 経由で `FlowgraphReloaded` WS push を発火するので、GUI の tree / diagnostics 再取得フローは δ-6d と同じ。
> - **GUI 統合（δ-7e）**: `FlowgraphTab.svelte` ツールバーに **Copy / Paste… / Export ZIP / Import ZIP…** の 4 ボタンを追加。Copy は選択ノードがあればそれ、なければ現在ファイル全体を fragment 化し、`navigator.clipboard.writeText` でクリップボードに書き込む（権限なしなら toast で fallback 通知）。Paste… と Import ZIP… は `FlowgraphShareDialog.svelte` を 2 モード（`paste` / `import_zip`）で使い回す。Import ZIP はファイル選択時に自動で `dry_run=true` のプレビューを取得 → ユーザーが確認後「Import」ボタンで本番適用、の 2 段階 UX。
> - **型整合**: Rust 側 `CopyRequest / CopyTarget / PasteRequest / PasteTarget / PasteOptions / PasteReport / FragmentDangling / ZipImportPreview / ZipImportReport` に 1:1 対応する TS 型を `gui/src/lib/types.ts` に追加。ZIP import は `ZipImportOutcome = Preview | Report` の tagged union で `kind` 判別。
> - **検証**: `cargo test --workspace` で新規 fragment/paste/zip_codec テスト群を含め全通し、`cargo clippy --workspace --all-targets -- -D warnings` で新規 warning ゼロ。GUI 側は `svelte-check` 0 errors / `vite build` 成功 / 新規コードに対して eslint 警告ゼロ（残る既存 5 件は本フェーズで触れていないファイル）。
> - **スコープ外（δ-8 以降）**: `notify` 連動の自動 watcher、完全 atomic な ZIP rollback（現状はファイル単位で `.bak` による可逆性のみ保証）、fragment の署名 / checksum 検証、複数選択の multi-node copy UI（単一ノード + 現在ファイル全体の 2 モードまで）。

---

## 9. Share / Export / Import プロトコル

### 9.1 選択範囲の種類

GUI で share/export する「選択範囲」は以下のいずれか:

- (S1) 単一ノード or 複数ノードの集合（任意の組み合わせ）
- (S2) 単一ファイル全体
- (S3) 単一フォルダ全体（サブフォルダ含む）
- (S4) 任意の複合（ノード + ファイル + フォルダ混在）

### 9.2 Fragment TOML フォーマット

選択範囲を TOML に切り出したときの schema。クリップボード用も単一ファイル export 用も同一。

```toml
[fragment]
schema = "vac-flowgraph-fragment/v1"
exported_at = "2026-04-17T12:34:56Z"
origin = "my-project/flowgraph"    # 任意: エクスポート元の目印
scope  = "nodes"                   # "nodes" | "file" | "folder" | "mixed"

# --- 内包する論理ファイル群 ---
# scope が "nodes" の場合も、便宜上 "__scratch__.flowgraph.toml" のような
# 仮想ファイル 1 つにまとめる。
[[fragment.files]]
path = "__scratch__.flowgraph.toml"     # scope=nodes の便宜ファイル
meta.title = "Copied nodes"

[[fragment.files.nodes]]
id = "tts_jp"
feature = "flowgraph.tts.voicevox"
position = [400, 200]
properties.host = "127.0.0.1:50021"

# --- 内包するエッジ ---
[[fragment.files.edges]]
from = "tts_jp:exec_out"
to = "bos_sink:exec_in"   # fragment 内で解決できない参照は dangling として保存

# --- fragment 内で解決できない外部参照 ---
[[fragment.danglings]]
kind = "edge_endpoint"
original = "../../ingress/twitch::main:content"
reason = "out_of_scope"
in_file = "__scratch__.flowgraph.toml"  # δ-7b 拡張: fragment 側の所属ファイル
edge_from = "../../ingress/twitch::main:content"
edge_to = "tts_jp:content"
external_side = "from"                    # "from" | "to"
```

- `fragment.files[]` は選択範囲に含まれるファイル（scope=file/folder/mixed）または仮想 scratch ファイル 1 つ（scope=nodes）。
- `fragment.danglings[]` は「選択範囲の外へ繋がっていたエッジ」の一覧。paste 時にユーザーが再マッピングするか破棄するかを選択する。
  - `kind` / `original` / `reason` は spec v1 で必須。
  - `in_file` / `edge_from` / `edge_to` / `external_side` は δ-7b で追加した **schema v1 の後方互換 optional 拡張**。paste 時に「どのエッジの、どちら側が外部参照だったか」を正確に再構築するために記録する。古い v1 fragment（これらを欠く）も読み込み可能で、その場合は dangling は unresolved のまま残る。
- paste 時の処理:
  1. 置き場所（ターゲットファイル or フォルダ）をユーザー選択
  2. ノード `id` 衝突を `on_conflict_node`（`suffix` / `skip` / `overwrite`）で解消（既定 `suffix`、例: `tts_jp` → `tts_jp_1`）
  3. ファイル衝突を `on_conflict_file`（`rename` / `skip` / `overwrite` / `merge`）で解消（既定 `rename`）
  4. 位置を `position_offset: [x, y]` でオフセット
  5. `remap: { "<original>": "<new_ref>" | null }` で dangling を UI から再マッピング（`null` 明示で drop、未指定なら unresolved に残す）

### 9.3 ZIP フォーマット

フォルダ構造保持での export/import。

```text
fragment.flowgraph.zip
├── manifest.flowgraph.toml          # fragment TOML の一部（メタ + scope + danglings）
└── flowgraph/                       # 元のフォルダ構造をそのまま内包
    ├── main.flowgraph.toml
    ├── ingress/
    │   └── twitch.flowgraph.toml
    └── tts/
        ├── main.flowgraph.toml
        └── jp_routing.flowgraph.toml
```

- `manifest.flowgraph.toml` は fragment TOML から `[[fragment.files]]` セクションだけを外した形（files は ZIP 内の実ファイルに委任）。
- ZIP 展開時の安全ルール（厳格）:
  - エントリパスに `..` / 絶対パス / シンボリックリンク を含むものは **拒否**。
  - `.flowgraph.toml`（および `.flowgraph.toml.disabled`）と `manifest.flowgraph.toml` は Flowgraph loader の対象ファイルとして展開・解釈。
  - **その他のファイル（`.md`, 画像, 音声等）も同梱許可**（δ-0 合意）。loader は非対象ファイルを解釈しないが、ZIP 内のフォルダ構造に従って展開される。`docs/manual/` 的な補助コンテンツをフラグメントに同梱できる。
  - ただし実行ファイル等の危険拡張子（`.exe`, `.dll`, `.bat`, `.ps1`, `.sh`, `.cmd` など）は安全のため **拒否**（spec ロック時に拒否リストを確定）。
- import は dry-run プレビュー必須: 衝突ファイル一覧、dangling 一覧、diff を表示してから確定。

### 9.4 Control API 追加

δ-7 で確定した実装仕様（spec 初稿からの差分は **太字**）。

#### `POST /api/v1/control/flowgraph/fragment/copy`

body（`CopyRequest`）:

```json
{
  "scope": "nodes" | "file" | "folder" | "mixed",
  "targets": [
    { "kind": "node", "fq_file": "main", "node_id": "tts_jp" },
    { "kind": "file", "fq": "tts/main" },
    { "kind": "folder", "path": "tts" }
  ],
  "origin": "my-project"
}
```

response（`CopyResponse`）:

```json
{
  "fragment_toml": "<TOML text>",
  "fragment": { /* 構造化 Fragment (§9.2) */ },
  "file_count": 2,
  "dangling_count": 1
}
```

#### `POST /api/v1/control/flowgraph/fragment/paste`

body（`PasteRequest`）:

```json
{
  "fragment_toml": "<TOML text>",
  "target": { "kind": "file", "fq": "main" } | { "kind": "folder", "path": "imported" },
  "options": {
    "on_conflict_node": "suffix" | "skip" | "overwrite",
    "on_conflict_file": "rename" | "skip" | "overwrite" | "merge",
    "position_offset": [x, y] | null,
    "remap": { "<dangling.original>": "<new_ref>" | null }
  }
}
```

response（`PasteResponse`）:

```json
{
  "report": {
    "written_files": [{ "fq": "...", "path": "..." }],
    "imported_nodes": [{ "fq_file": "...", "original_id": "...", "new_id": "..." }],
    "skipped_nodes":  [{ "fq_file": "...", "original_id": "...", "reason": "..." }],
    "skipped_files":  ["..."],
    "unresolved_danglings": [ /* §9.2 の FragmentDangling */ ],
    "backups": ["..."]
  },
  "reload_ok": true,
  "diagnostics_count": 0,
  "node_count": 42
}
```

paste 成功後は内部で reload を走らせ `FlowgraphReloaded` WS push を発火する。

#### `POST /api/v1/control/flowgraph/export/zip` **（spec 初稿の GET から POST へ変更）**

- body は `CopyRequest` そのもの（`fragment/copy` と共通）。targets を URL query で送る GET 方式は `folder` 等の複雑 target で破綻するため POST + JSON body 固定。
- response: `Content-Type: application/zip`, `Content-Disposition: attachment; filename="fragment.flowgraph.zip"`。

#### `POST /api/v1/control/flowgraph/import/zip`

- **body は raw ZIP バイナリ**（`Content-Type: application/zip`）。`actix-multipart` 依存を持ち込まないための割り切りで、GUI は `fetch` の `body: Blob` で素直に送れる。
- query:
  - `dry_run=true|false`（既定 `false`）
  - `target_prefix=<root 相対パス>`（既定 `imported`、空文字で root 直下）
  - `on_conflict_node=suffix|skip|overwrite`（既定 `suffix`）
  - `on_conflict_file=rename|skip|overwrite|merge`（既定 `rename`）
- response（`ZipImportOutcome` の tagged union、`kind` で分岐）:
  - `dry_run=true` → `{ "kind": "preview", manifest, file_count, entries: [{ zip_path, dest_path, size }], danglings, target_prefix, conflicts, companions }`
  - `dry_run=false` → `{ "kind": "report", manifest, written_files, written_companions, danglings_unresolved, target_prefix }`
- 詳細な dangling remap や複雑な衝突戦略を指定したい場合は、ZIP から fragment を取り出した後に `fragment/paste` を直接叩く運用とする（ZIP はバルク輸送用途として切り分け）。

### 9.5 ファイル単位の CRUD

- `GET /api/v1/control/flowgraph/tree` — フォルダ/ファイルツリー + 各ファイルのメタ
- `GET /api/v1/control/flowgraph/file/<path>` — 単一ファイルの nodes/edges JSON 化
- `PUT /api/v1/control/flowgraph/file/<path>` — 単一ファイル丸ごと更新（format-preserving を保つため、GUI 側差分を backend で merge する方式を δ-5 で詳細化）
- `POST /api/v1/control/flowgraph/file` — 新規作成（body: `{ path }`。初期内容は **空テンプレート**（`[meta]` 最小ヘッダのみ）、任意で `initial_toml` 上書き可。δ-0 合意）
- `DELETE /api/v1/control/flowgraph/file/<path>` — 削除（`.bak` 生成）
- `POST /api/v1/control/flowgraph/file/<path>/open-external` — 既存 modify_open.rs と同等の OS 既定エディタ起動
- `GET /api/v1/control/flowgraph/node-catalog` — 全 NodeSpec JSON

---

## 10. 将来構想（スコープ外の記録）

### 10.1 AI Persona ノード化

- δ 完了時点では `AiService` は別エンジン。将来は `flowgraph.ai.persona_generate` ノードとして Flowgraph に統合する構想。
- 課題: セッション状態（履歴、コンテキスト、tools）の DAG ノードとしての表現、streaming 応答の exec チェーン上での扱い、AiPersonaConf の PropertySpec 化。
- 先行検討として δ-0 で `AiService` → Flowgraph の境界 API を整理しておく（別ドキュメント予定）。

### 10.2 辞書フォーマット TSV 化（η フェーズで完了）

- ~~現在 `dictionary.chat.txt` / regex CSV など複数フォーマット。Phase ε で TSV 統一予定。~~
- ~~δ-4a で Modify を Dictionary + Regex に分解する際、既存フォーマットはそのまま尊重。TSV 統一は後続フェーズ。~~
- **2026-04-23 更新**: Phase η（Dictionary/Table Unification）にて、辞書と正規表現を 1 本の 11 カラム TSV に統一し、`SocketType::Table` + `flowgraph.table.*` / `flowgraph.dictionary.*` で扱えるようになった。V1 の loose 2 列形式は `flowgraph.table.load_tsv` が `mode=legacy_loose` / `auto` で吸収する。詳細仕様は [phase-eta-dictionary-unification.md](phase-eta-dictionary-unification.md) §3 / §4 / §8（migration）を参照。

### 10.3 ホットリロード

- §8.5 参照。δ-5 で loader 実装と併せて可否判断。

### 10.4 プロパティのポート化

- ノードインスタンスのプロパティを、上流ノードから動的に上書きできる機能。UE Blueprint の「Expose on Spawn / Variable Get」相当。δ スコープ外。

### 10.5 Flowgraph を独立プログラミング言語へ

- 発想（未検討メモ）: VAC 内部 DSL として育てた Flowgraph を、**データフロー型ワークフロー言語 → 汎用プログラミング言語 "Flowgraph"** として切り出す構想。
- 足場となる既存資産:
  - 型システム（SocketType: Bool/Int/Float/String/Json/List/Map/Exec）は既に言語として自足的な粒度。
  - Pure / Stateful / Effectful の 3 trait 分類が「純粋関数 / ステートマシン / I/O」という言語設計の古典的な 3 分類に綺麗に対応している。
  - Exec 遅延評価ハイブリッド実行モデル（§5）は、そのままコアセマンティクスとして公開可能。
  - TOML フォーマットはテキスト IR として流用でき、GUI ⇔ テキストの両面編集も担保済み。
- 想定ロードマップ（まだ妄想段階）:
  1. VAC 外に持ち出せる `flowgraph-core` crate を切り出し、VAC はその利用者の一つとする。
  2. Node カタログをモジュール単位に整理し、Flowgraph Standard Library を定義（string / json / list / map / math / io / net / time …）。
  3. WASM ターゲット: `flowgraph-core` を `wasm32-unknown-unknown` でビルドし、ブラウザ / エッジランタイムで Flowgraph を実行可能にする。Flowgraph TOML をそのまま WASM にコンパイル / または JIT 実行するランタイム。
  4. FFI: Rust / C / C++ / Python / JS から `flowgraph-core` を呼び出し、Flowgraph をスクリプト言語として埋め込める状態にする。
  5. 汎用言語化: ユーザー定義ノード（サブグラフ = 関数定義）、型ジェネリクス、ADT 的な拡張、コンパイラ最適化（データ DAG の定数畳み込み、dead-node elimination、並列性抽出）。
- 注目点:
  - ビジュアル編集とテキスト編集が等価、という世界観は既存のデータフロー言語（LabVIEW, Max/MSP, Unreal Blueprint）との差別化要素になる。
  - 「Pure / Stateful / Effectful の型付き分離」と「Exec eager / Data lazy のハイブリッド評価」を言語レベルで保証するのは珍しく、効果システム入門言語として独自のポジションを取れる可能性。
  - 成功すれば VAC は "Flowgraph のキラーアプリ" として後から語られる立場になる。
- 現状は**完全に将来のアイデア・メモ**に過ぎない。δ 完了と v1.0 が先。ε・ζ などのフェーズでも、まずは VAC としての実用性と安定性を優先する。

---

## 11. V1 → Flowgraph 対応マップ

### 11.1 型・概念

| V1 概念 | Flowgraph 対応 |
| ------- | -------------- |
| `Processor` trait (`src/processor/mod.rs:1-65`) | `FlowgraphNode` trait |
| `ProcessorKind` enum (`src/processor/mod.rs:67-88`) | NodeRegistry（カタログ） |
| `is_channel_from(&str)` | 廃止（エッジ接続が実行条件） |
| `channel_from` / `channel_to` | 入力ポート / 出力ポートの接続 |
| `ChannelDatum.content` (`src/state/channel_datum.rs:14`) | 出力ポート `content: String` |
| `ChannelDatum.meta` (`src/state/channel_datum.rs:23`) | 個別ポート（よく使うもの）+ `raw_json: Json` |
| `ChannelDatum.flags` (`src/state/channel_datum.rs:15`) | 出力ポート `flags: List<String>` |
| `ChannelDatum.attachments` | 出力ポート `attachments: List<Json>`（attachment 型は将来型化） |
| `ChannelDatum.source` | 個別ポート `source_kind: String` / `source_actor: String` |
| `ProcessorConf` superset | 各ノードの `PropertySpec[]` |
| `conf.toml` `[[processors]]` | `flowgraph/**/*.flowgraph.toml` |
| `conf.toml` `[[ai.personas]]` | **δ では維持**（ε 以降で Flowgraph 統合検討） |
| `CompletedAnd::Next/Break` | exec チェーン続行の有無（ノード内で exec 出力を発火しない = Break 相当） |

### 11.2 V1 プロセッサ × 13 種 → Flowgraph ノード対応

| V1 feature | Flowgraph feature | 分解 | 担当フェーズ |
| ---------- | ----------------- | ---- | ------------ |
| `command` | `flowgraph.command.match` | そのまま | δ-4a ✅ |
| `modify` | `flowgraph.dictionary.replace` + `flowgraph.regex.replace` | 2 ノードに分解 | δ-4a ✅ |
| `dictionary_command` | `flowgraph.dictionary.command` | そのまま | δ-4a ✅ |
| `screenshot` | `flowgraph.screenshot.capture` | そのまま（単一クロップ化） | δ-4b ✅ |
| `ocr` | `flowgraph.ocr.recognize` | そのまま（単一 source 化） | δ-4b ✅ |
| `gas_translation` | `flowgraph.translate.gas` | そのまま | δ-4b ✅ |
| `libre_translation` | `flowgraph.translate.libre` | そのまま（埋め込みサーバ管理は外出し） | δ-4b ✅ |
| `os_tts` | `flowgraph.tts.speak` + `engine="os"` | **統合 TTS ノード + 動的 driver 切替**に設計変更 | δ-4c ✅ |
| `coeiroink` | `flowgraph.tts.speak` + `engine="coeiroink"` | 同上（`/v1/synthesis` 固定へ簡素化） | δ-4c ✅ |
| `aivis_speech` | `flowgraph.tts.speak` + `engine="aivis_speech"` | 同上（VOICEVOX 互換 driver 共有） | δ-4c ✅ |
| `voicevox` | `flowgraph.tts.speak` + `engine="voicevox"` | 同上 | δ-4c ✅ |
| `bouyomichan` | `flowgraph.tts.speak` + `engine="bouyomichan"` | 同上（**TCP プロトコル port 50001 に変更**、RemoteTalk.exe 依存を排除） | δ-4c ✅ |
| （V1 未対応） | `flowgraph.tts.speak` + `engine="voicepeak"` | 新規: VoicePeak CLI を spawn し narrator + emotion CSV で合成。exe は `endpoint` / `[voicepeak]` / OS 既定（`extra.executable` は非推奨互換） / `extra.emotion` 対応 | δ-4c.1 ✅ |
| `twitch_out` | `flowgraph.twitch.chat_send` + `flowgraph.twitch.validate_token` + `flowgraph.twitch.user_id_by_login` + `flowgraph.util.rate_limit` | **単機能ノードへ分解**。V1 の「起動時 token 取得→broadcaster_id 解決→rate limit→send→401 で refresh 再送」は graph-level（`state.latch` + `on_error` 分岐）で組み直す | δ-4d ✅ |

### 11.3 V1 Ingress × 3 種 → Flowgraph ノード対応

| V1 | Flowgraph feature | 担当フェーズ |
| -- | ----------------- | ------------ |
| Twitch (chat / eventsub) | `flowgraph.ingress.twitch` | δ-3 |
| Web Input (`/input/*`) | `flowgraph.ingress.web_input` | δ-3 |
| Voice (Vosk / Whisper) | `flowgraph.ingress.voice` | δ-3 |

---

## 12. マイグレーション（δ-8）

### 12.1 δ-8 のスコープ転換（2026-04）

δ-0 spec 時点では「v1 `conf.toml` を全自動で v2 にロスレス変換する CLI」を想定していた。
δ-8 着手時のレビューで、既存 `conf.toml` の構造は v1→v2 の差分があまりに大きく **「変換チャレンジ」** の側面が強いと確認し、δ-8 の主目的を下記のとおり再定義した:

1. **v2 としてゼロから配布物を組み直す**（δ-8a / δ-8b / δ-8c）
   - ルート `conf.toml`（v2 テンプレート）
   - `flowgraph.example/<topic>/*.flowgraph.toml`（feature 別サンプル）
   - `conf.example-*.toml`（外部サービス向けの設定リファレンス）
   - `docs/manual/`（index / quickstart / conf-reference / node-catalog / tutorials）
2. **v1 ユーザーの引っ越し支援として migrate CLI を提供**（δ-8d）
   - あくまで best-effort の「変換チャレンジ」ツール。
   - 完全互換は目標にせず、**骨格 Flowgraph と warning/TODO コメント**で手動補正を促す。

旧 spec（§12.1〜§12.3）にあった「`conf.toml.v1.bak` に退避して上書き」「`channel_from` / `channel_to` 連鎖を線形エッジに推論」等の方針は **破棄**。現行実装（δ-8d）は以下に従う。

### 12.2 配布物（δ-8a〜δ-8c で完成）

- `conf.toml` — v2 のルートテンプレート。`[[processors]]` を含まないことをテスト（`conf::tests::root_conf_has_no_processors`）で強制。`flowgraph_dir = "flowgraph.example"` を含む。
- `flowgraph.example/` — 13 サブディレクトリ（`chat-echo` / `chat-filters` / `ocr-screencap` / `openai-persona` / `translate-multilang` / `tts-os` / `tts-voicevox` / `tts-aivis-speech` / `tts-coeiroink` / `tts-bouyomichan` / `twitch-echo` / `twitch-chat-send` / `twitch-moderation` / `voice-input`）。
- `conf.example-*.toml` — feature 別の参考設定（TTS / OCR / Twitch / OpenAI / translate / voice / chat-filters / control-api / run-with）。
- `docs/manual/`
  - `index.md` — マニュアルのエントリ
  - `quickstart.md` — 起動〜最小フロー実行
  - `conf-reference.md` — `conf.toml` の全キー
  - `node-catalog.md` — **`src/flowgraph/docs.rs::render_node_catalog_md`** が NodeRegistry から自動生成。`BLESS_NODE_CATALOG=1 cargo test node_catalog_md_up_to_date` で regen。
  - `tutorials/*.md` — `flowgraph.example/` 各トピックに 1:1 対応。

### 12.3 migrate CLI（δ-8d）

#### 12.3.1 起動方法

```powershell
virtual-avatar-connect.exe --migrate --conf conf.toml [--migrate-out-dir migrated] [--migrate-strict]
```

- `--migrate` を付けると `execute_special_modes_without_conf` 内で `crate::migrate::migrate_conf_file` を呼び、終了する（通常起動には入らない）。
- `--migrate-out-dir <path>` — 出力先（省略時 `./migrated/`）。**既存ディレクトリがあると拒否**（事故防止。ユーザーが別名 or 削除を選ぶ）。
- `--migrate-strict` — warning でも exit 1。通常は warning は stdout サマリ + `MIGRATION-NOTES.md` に残すだけ。

#### 12.3.2 出力物

```
<out>/
  conf.toml              # [[processors]] を除去し flowgraph_dir = "flowgraph" を追加したもの
  flowgraph/
    <topic>/main.flowgraph.toml  # feature 別に 1 フラグメント（重複時は <topic>-1 / <topic>-2 ... ）
  MIGRATION-NOTES.md     # 変換サマリ（converted / warn / error 一覧）
```

**入力 `conf.toml` は一切変更されない**（δ-0 時代の `conf.toml.v1.bak` 方式は破棄）。v1 conf は git / バックアップでユーザーが管理する前提。

#### 12.3.3 変換対象 feature とマッピング

| v1 `feature`                          | 出力 topic              | 骨格 Flowgraph                                                                                 |
| ------------------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------- |
| `modify`                              | `chat-filters`          | `ingress.web_input → dictionary.replace → util.log`（辞書配線は手動 TODO）                      |
| `command`                             | `chat-filters-command`  | `ingress.web_input + literal.string("/") → command.match → util.log` × 2（cmd / other 分岐）   |
| `dictionary-command`                  | `chat-dictionary-command` | `ingress.web_input → dictionary.command → util.log`                                          |
| `gas-translation`                     | `translate-gas`         | `ingress.web_input + literals(script_id/source/target) → translate.gas → util.log`             |
| `libre-translation`                   | `translate-libre`       | 同上（`translate.libre` + base_url literal）                                                    |
| `screenshot`                          | `screenshot`            | `ingress.web_input + literal(title) → screenshot.capture → util.log`                           |
| `ocr`                                 | `ocr`                   | `ingress.web_input + literal(lang) → ocr.recognize → util.log`                                 |
| `os-tts`                              | `tts-os`                | `ingress.web_input + engine/voice literal → tts.speak → util.log`                              |
| `voicevox` / `aivis-speech`           | `tts-voicevox` / `tts-aivis-speech` | 同上 + `endpoint` literal、`voice = "<uuid>:<style_id>"` or `style_id` 単体       |
| `coeiroink`                           | `tts-coeiroink`         | 同上（`voice = "<uuid>:<style_id>"`）                                                           |
| `bouyomichan`                         | `tts-bouyomichan`       | 同上（`endpoint = "<address>:<port>"`、`voice = <voice>`）                                      |
| `twitch`                              | `twitch-echo`           | `ingress.twitch → util.log`（`[twitch]` / `[twitch.eventsub]` / `[twitch.moderator]` は保持） |
| `twitch-out`                          | `twitch-chat-send`      | `ingress.web_input + 4 × literal(token/client_id/broadcaster_id/sender_id) → twitch.chat_send` |
| `web-input`                           | `web-input`             | `ingress.web_input → util.log`                                                                 |
| `voice`                               | —                       | **変換しない**。`[[processors]]` のまま保持 + warning（δ-9 で `[voice]` サービス化予定）         |
| 上記以外                              | —                       | warning で skip。ユーザー手動対応。                                                              |

- 同一 feature が複数あれば、2 個目以降は `<topic>-1` / `<topic>-2` とサフィックスで衝突回避。
- 各フラグメントは `[meta] title / description / tags` を付け、先頭コメントに `# migrated from processors[<idx>] feature="…"`、旧 `id` / `channel_from` / `channel_to` を明記。
- 未変換の詳細パラメータ（`speed_scale` / `crops` / `response_mod` / `rate_limit_per_30s` 等）は `# TODO:` コメントで残す。
- トークン類（Twitch access_token / client_id / broadcaster_id / sender_user_id）は literal プレースホルダ（`"TWITCH_ACCESS_TOKEN_HERE"` 等）で書き出し、差し替えを促す。

#### 12.3.4 ロスレス保証と strict モード

- 単純な linear pipeline でも **`channel_from` / `channel_to` の連鎖からエッジ推論は行わない**（spec §12.2 旧 2. は破棄）。各 processor はそれぞれ独立した骨格フラグメントになるため、v1 の trigger 合成（fan-in / fan-out）は **必ず手動再構築**が必要。
- すべての「変換不能」「警告相当」な情報は `MigrationReport` に記録され、stdout サマリと `MIGRATION-NOTES.md` の両方に列挙される。
- `--migrate-strict` を付けると `MigrationReport::has_errors()` が `Warning` も error として扱い、exit 1。CI で「v1 conf はもう出てはならない」ことをガードする想定。

#### 12.3.5 実装モジュール

- `src/migrate/mod.rs`
  - `migrate_conf_file(input: &Path, out_dir: &Path, strict: bool) -> Result<MigrationReport>`
  - `migrate_conf_str(toml: &str, out_dir: &Path, strict: bool) -> Result<MigrationReport>`（テスト用）
  - `MigrationReport { entries, written_files, strict }` — `Converted` / `Warning` / `Error` の 3 種エントリ。
  - `render_flowgraph_for(feature, &Table, idx) -> Option<String>` — 既知 feature のみ Some、未対応は None。
  - ユニットテスト 6 件（minimal / voice 保持 / unknown feature / strict / voicevox 具象化 / 重複 feature サフィックス）。

---

## 13. V1 除去（δ-9）

> **δ-9 実装ノート (v0.9.0)**: 本フェーズは当初計画で「V1 完全除去 + v1.0 major bump」としていたが、V1 processor 層が 29 ファイル / ~280KB と巨大で単一セッションでの安全な除去が非現実的と判明。方針を「**Part A-C を先出しして Flowgraph runtime を並走稼働させ、V1 除去 (Part D) は δ-9.2 へ分割**」に変更した。
>
> **完了 (v0.9.0)**:
> - **Part A**: `FlowgraphRuntime` を worker spawn 型に再設計。`src/flowgraph/spawn.rs` で `FlowgraphProgram` を独立 tokio タスクに move 所有させ、`TriggerHandle` + `shutdown_tx` のみ `State` に保持する。`ExecCtx.state_handle: Option<Weak<RwLock<State>>>` を追加し、Effectful ノードから `State::push_channel_datum` 等を呼べるようにした。`State::new` は `load_and_spawn` 経由で runtime を常時起動。
> - **Part B**:
>    - `flowgraph.ingress.web_input` / `voice` / `twitch` に property を追加（`path` / `method` / `body_format` / `fixed_channel`、`engine` / `model_path` / `sample_rate` / `language_code` / `grammar_json`、`mode` / `channels` / `access_token` / `client_id` / `broadcaster_id` / `login` 等）。
>    - `LoadedNodeMeta.properties: InputMap` を追加し、bridge がロード時の property 値を読めるように改修。
>    - `src/bridges/` モジュール新設。`web_input` ブリッジは完全実装（actix-web 統合 + 4 ポート + plain/json/form decode + trigger 送信、テスト 4 本）。`voice` / `twitch` ブリッジは property 抽出のみで、実ワーカーは V1 実装共用のまま（spawn 時に暫定注意ログを出す）。
>    - lib.rs `run_services` に bridge 収集 + `flowgraph.ingress.web_input` 経路のルート登録を追加。
> - **Part C**: `flowgraph.channel.emit`（EffectfulNode）を新設。`exec_in` / `channel` / `content` / `is_final` / `source_actor` の 4 ポートと `exec_out` / `on_error` を持つ。`ExecCtx.state_handle` 経由で `State.push_channel_datum` を呼び出し、V1 ws / browser-output 配信系へ終端を繋げる。フォールバック channel / state_handle 不在 / upgrade 失敗で `on_error` 発火。単体テスト 3 本。
>
> **D.1 (v0.9.0 過渡期、完了後に削除済み)**: `conf.v1_processors_enabled: Option<bool>` を追加し force_on ゲートとして一度介在させたが、D.2/D.3 の実削除と同時にフィールドごと除去した。履歴メモのみ。
>
> **δ-9 D.2/D.3/D.4/D.5 (v0.9.x、完了)**:
> - **D.2/D.3**: V1 processor 層の実装ファイル 11 本 (`aivis_speech.rs` / `bouyomichan.rs` / `coeiroink.rs` / `command.rs` / `dictionary_command.rs` / `gas_translation.rs` / `libre_translation.rs` / `modify.rs` / `os_tts.rs` / `voicevox.rs` / `twitch_out.rs`) を一括削除。`Processor` trait / `ProcessorKind` enum / `ProcessorRuntime` / `init_processors` / `dispatch_processors_for_incoming` / `State::processors` / `State::processor_runtimes` / `conf.v1_processors_enabled` を除去。`src/processor/ocr/` と `src/processor/screenshot/` は Flowgraph ノードが参照するユーティリティモジュールとして残置（`Processor` trait 実装は剥離済み）。
> - **D.4**: Control API の V1 エンドポイントを除去: `modify_content.rs` / `modify_entries.rs` / `modify_open.rs` / `node_config.rs` を削除。`reload.rs` から `modify_files` target を削除し AI persona 限定に縮退。`actions.rs` の `PauseTarget` から `Processor` / `Processors` variant を除去。`dto.rs` から `processors: Vec<ProcessorSummary>` を除去し `schema=1 → 2` に bump。
> - **D.5**: GUI から Pipeline タブおよび関連 Svelte コンポーネント群 (`PipelinePanel.svelte` / `ProcessorNode.svelte` / `IngressNode.svelte` / `NodePropertiesPanel.svelte` / `DictionaryEditorModal.svelte` / `RegexEditorModal.svelte` / `AiNode.svelte` / `pipelinePulse.svelte.ts`) を削除。`#pipeline` URL は `flowgraph` へフォールバック。
> - **CLI diagnostic**: `--coeiroink-speakers` / `--aivisspeech-speakers` / `--voicevox-speakers` / `--test-os-tts` は v0.9.x で一時停止しログ出力のみ。`/status` HTML から CoeiroInk セクションも除去。δ-9.3 で Flowgraph TTS ドライバ経由で再実装予定。
> - **Windows OCR/Screenshot**: `windows` crate 0.62 API 変更 (`IAsyncOperation::get()` / `Error::from_win32()` 失効) に伴い、`src/processor/ocr/mod.rs::recognize` は暫定で `anyhow::bail!`、`src/processor/screenshot/windows.rs` は `E_FAIL` ベースの汎用エラーへフォールバック。δ-9.3 で `windows-future` 経由の再実装予定。
>
> **延期 (Part E / 次期)**: Part E (conf.local 完全移行) はユーザーが手動試運転しながら進行中。v1.0 major bump は Part E 完了と δ-9.3 (windows/TTS diagnostic 復活 + docs 刷新) 完了後に実施予定。

### 13.1 削除済み（v0.9.x）

- `src/processor/` 配下の V1 実装ファイル 11 本（[完了] D.2/D.3）
- `Processor` trait / `ProcessorKind` / `CompletedAnd` / `ProcessorRuntime` / `dispatch_processors_for_incoming` / `init_processors`（[完了] D.2/D.3）
- `State::processors` / `State::processor_runtimes` / `State::v1_processors_enabled` フィールド（[完了] D.2/D.3）
- `Conf::v1_processors_enabled`（[完了] D.2/D.3）
- Control API の V1 エンドポイント（`/modify/*` / `/node_config` / `reload modify_files` target / `PauseTarget::Processor[s]`）（[完了] D.4）
- GUI Pipeline タブおよび関連 Svelte コンポーネント群（[完了] D.5）

### 13.2 未着手 / 継続

- `src/conf/processor_conf.rs` の完全除去（`ProcessorConf` はパース互換のため暫定残置 + warning）
- `src/bridges/voice.rs` / `twitch.rs` の完全実装（ingress Flowgraph ノード → `TriggerHandle.send` の実装 + V1 `voice_*` / `twitch*` の整理）
- `conf.example-*.toml` / Bruno collection / `docs/` の Flowgraph 前提への刷新
- README の feature 一覧を `NodeSpec.feature` 一覧へ差替え
- Windows OCR / Screenshot の `windows-future` 対応 (δ-9.3)
- CoeiroInk / AivisSpeech / VOICEVOX / OS-TTS の diagnostic CLI と `/status` セクションを Flowgraph TTS ドライバ経由で再実装 (δ-9.3)

### 13.3 バージョンとドキュメント

- `Cargo.toml` `version = "0.8.0"` → `"0.9.0"`（δ-9 Part A-C、minor bump、実施済み）。
- `"1.0.0"` major bump は Part E 完了 + δ-9.3 完了後に実施予定。
- `CHANGELOG.md` に δ-9 Part A-C の Breaking / 追加、および δ-9 D.2-D.5 の追加 Breaking changes を列挙済み。

---

## 14. δ-0 合意事項（spec lock）

δ-0 レビューの結果、以下 7 件が確定した。spec 本文はこの合意に従って整合済み。以降の δ-1〜δ-9 実装はこの合意を前提に進める。

### 14.1 合意済み一覧

- [x] **Map<K, V> の key 型**: **String 限定**（選択肢 X）。SocketType は `Map(Box<SocketType>)` と value 型のみパラメタ化。§2.1 / §2.2 / §2.3 に反映済み。論点詳細は §14.2。
- [x] **必須入力未接続時の挙動**: **error 停止**（安全側）。§5.2 / §8.2 に反映済み。
- [x] **ホットリロード**: δ-5 での実装は必須とせず、独立性が高い場合は δ-6 以降に回す。§8.5 で現方針維持。
- [x] **ZIP import の non-flowgraph ファイル**: **同梱許可**。Flowgraph loader は `.flowgraph.toml` 系のみ解釈、非対象ファイルはそのままフォルダに展開。ただし実行系危険拡張子（`.exe`/`.dll`/`.bat` 等）は拒否。§9.3 に反映済み。
- [x] **新規ファイル作成時の初期テンプレート**: **空ファイル**（`[meta]` 最小ヘッダのみ）。サンプルは `docs/manual/` 配下の Example 文書で充実化。§9.5 API に反映済み。
- [x] **`main.flowgraph.toml` が存在しないフォルダへの `folder/` 参照**: **warning 表示**、GUI は「空の main 作成」1 クリック解消ボタンを提供。§6.3 に反映済み。
- [x] **Control API の認証スコープ**: **既存 `/api/v1/control/*` スコープと完全共有**（選択肢 A）。既存 `control_api_auth` ミドルウェアがそのまま Flowgraph API にも効く。論点詳細は §14.3。
- [x] **ZIP import の body size / ロールバック**: δ-0 では仕様ロックせず、**δ-7 Share/Export/Import 実装時に決定**。方針のみ記述（§14.4）。

### 14.2 論点ノート: Map<K, V> の key 型（参考）

**選択肢**:

- **(X) String 限定**
  - SocketType を `Map(Box<SocketType>)` と value 型のみパラメタ化
  - TOML 表現（テーブル key は String のみ）と完全整合
  - Rust 実装は `BTreeMap<String, SocketValue>`、Float の NaN 問題も関係なし
  - GUI の Map エディタは key 入力欄が常に TextField で済み UI が単純
  - カバー範囲: HTTP Headers (`Map<String, String>`), LLM tools parameters (`Map<String, Json>`), labels, env vars — 実運用のほぼ全て
- **(Y) String | Int | Bool のみ**
  - TOML では quoted key `"42" = ...` / `"true" = ...` で書けるが、視覚的に読みづらい
  - key の equality/hash 設計が僅かに面倒、ランタイムの key 型判別も分岐
  - 非 String key のユースケース（id 配列の dictionary 等）は存在するが、いずれも Json で代替可能
- **(Z) 任意 SocketValue**
  - Float / List / Map をキーにできるが、NaN 問題、ネスト key の equality、TOML 表現手段なし（array-of-table で擬似表現するしかない）
  - 実装コスト・ユーザー混乱いずれも大
- **(W) Map 型を廃止、Json に統合**
  - 「辞書的データは `Json` で扱え」に統一。SocketType が 1 種減る
  - ただし `List<T>` は残す前提との非対称（List だけ型付きで Map は unspecified）、GUI の「キー列挙 UI」が失われる
  - 型安全性・GUI 表現力が一段下がる

#### 確定: (X) String 限定

- 理由: TOML / JSON / HTTP Headers / LLM tools parameters / YAML / 環境変数 など実運用で遭遇する Map はほぼ全て string key。対称性は `List<T>` と `Map<T>` で十分。Json 型との使い分けは「エントリが同一 value 型で列挙できる辞書 = Map」「スキーマ流動の任意ツリー = Json」で明快。
- 非 String key 要求が出たら Json 型で代替、または後続 Phase で (Y) へ拡張（互換破壊なし）。

### 14.3 論点ノート: Control API の認証スコープ（参考）

#### 前提（既存実装）

- `/api/v1/control/*` 全体に `control_api_auth` ミドルウェアが wrap されている ([src/web_interface/control/mod.rs](../../src/web_interface/control/mod.rs))
- 既定ポリシー: loopback (127.0.0.1 / ::1) は無認証許可、non-loopback (LAN) は `Authorization: Bearer <token>` 必須
- `[control_api]` セクションで `require_token_for_loopback` / `require_token_for_non_loopback` などを運用時に調整可能
- Bearer token 値は設定で上書き可、未設定時は起動時自動生成してファイルに書き出す（GUI が自動取得）

#### 選択肢

- **(A) 既存 `/api/v1/control/*` スコープ内に `/api/v1/control/flowgraph/*` を置き、ミドルウェアも既存と完全共有**
  - 利点: 実装コスト最小。GUI はすでに Bearer 提示ロジックを持つので配線不要。運用者の設定面も 1 ヶ所で完結。既存の profile/run_with/node_config/ai_persona 編集 API と同じセキュリティモデル（Flowgraph 編集は破壊力として同等）
  - 欠点: Flowgraph だけ強いポリシーにしたい運用者に対して個別調整できない
- **(B) スコープは共有しつつ、write 系（PUT/POST/DELETE）にのみ追加チェック**
  - 例: loopback でも Flowgraph write は Bearer 必須化、破壊的操作（delete file / import zip --overwrite）に `X-Confirm-Destructive: true` ヘッダ必須化
  - 利点: 安全側に倒せる。既存読み取り API の手触りを壊さない
  - 欠点: Flowgraph write だけ一手増えて GUI 側ロジックが微増
- **(C) Flowgraph 専用スコープ `/api/v1/flowgraph/*` + 専用ミドルウェア**
  - 利点: policy を完全に独立調整可能
  - 欠点: conf 二重化、既存 API との対称性喪失、実装コスト大

#### 確定: (A)

- 理由:
  - Flowgraph 編集の破壊力は既存 profile/run_with/node_config 編集と同等（どれもアプリの挙動を書き換える）。別扱いする根拠が薄い
  - 既存 `[control_api]` ポリシーで loopback 要認証化は既に切り替え可能。「Flowgraph だけ強く」したい運用者はシステム全体を強くすれば良い
  - GUI の既存 `ControlApiClient` がそのまま流用できる
- (B) の追加チェックは、**必要になったら後付けで追加**できる（ミドルウェア単位で重ねるだけ）。spec 段階で入れておくと YAGNI

### 14.4 ZIP import の body size / ロールバック（δ-7 実装で確定）

δ-7 実装時に以下の方針で確定した:

- **body size 上限**: actix-web の `web::Bytes` は既定で `PayloadConfig` の上限（標準 256 KiB）に縛られる。`src/web_interface/control/mod.rs` の `configure` で `PayloadConfig::new(64 * 1024 * 1024)` に引き上げ、**初期値 64 MiB** とした。将来 `[control_api.zip_body_limit_bytes]` で可変化する予定（δ 範囲外）。
- **安全ルール（§9.3）を最初に全エントリでチェック**: パスに `..` / 絶対パス / Windows ドライブ / null byte を含むもの、Unix シンボリックリンク、および拒否拡張子（`.exe`, `.dll`, `.bat`, `.ps1`, `.sh`, `.cmd`, `.com`, `.scr`, `.msi`, `.app`, `.apk`, `.jar`）は **パース段階で拒否**し、1 バイトも書かずに 400/422 を返す。
- **展開中の atomic 性**: dry_run=true でまずプレビューを返し、GUI 側で確認 → dry_run=false で本番実行の 2 段階。本番側は ZIP → メモリ展開 → 衝突検出 → `fragment/paste` エンジンにそのまま委譲、という流れで、paste エンジン側が既存ファイルの `.bak` を作ってから rewrite する。`.bak` で「ひとつ前の状態」に戻せることを保証する（フル atomic では無いが、**ファイル単位では可逆**）。
- **部分失敗のロールバック**: `fragment/paste` はファイル単位で順次書き込むため、途中失敗時は成功済みファイル分は `.bak` 経由で手動復旧できる状態を残す。ZIP 全体を一時フォルダに展開してから差し替える完全 atomic 方式は、ファイル数が多い大規模フォルダでコストが高く、また companion（非 flowgraph ファイル）を含むケースで `.bak` 互換の可逆性が揃わないため採用しなかった。ここでトラブルが出たら δ-7 後追いで強化する。

---

## 15. 参考

- 本 spec は以下の既存コードの構造を前提に設計した:
  - プロセッサ列挙: [src/processor/mod.rs](../../src/processor/mod.rs)
  - ChannelDatum: [src/state/channel_datum.rs](../../src/state/channel_datum.rs)
  - 既存 TOML 編集基盤: `src/web_interface/control/node_config.rs`, `src/web_interface/control/run_with.rs`
  - イベントバス: `src/web_interface/ws.rs`

- UE Blueprint の概念（入出力ピン / exec pin / DAG）を参考にしているが、本プロジェクトの Flowgraph は UE Blueprint とは別実装の独自モデルである。

---

## 16. 後続 Phase

- **Phase ε**（[phase-epsilon-shutdown-and-tauri.md](phase-epsilon-shutdown-and-tauri.md)） — 終了処理の統合（`ShutdownBroker`）と Tauri ネイティブウィンドウ移行。δ-10 としてではなく別立てで扱う（δ は Flowgraph に閉じる）。
