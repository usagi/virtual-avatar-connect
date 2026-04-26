# Phase λ — Flowgraph: Enum 型システム + ライブラリ再利用

本書は **Phase λ（lambda）** の設計正本である。Enum（閉集合文字列）、ユーザ定義 `[[enums]]`、ライブラリ境界ノード、`[meta]` 拡張、ライブラリ依存グラフと循環検出の方針をここに集約する。実装はサブフェーズ λ-0 … λ-5 に分割し、[`roadmap.md`](../roadmap.md) の **Completed**（および将来の λ+）と照合する。

---

## 1. 役割分担（Phase υ との関係）

| 概念 | Phase λ（本書） | Phase υ（GUI subgraph 等） |
|------|-----------------|---------------------------|
| 再利用単位・型境界・ファイル間依存 | **主担当** | 参照のみ |
| エディタ上のグループ / Undo / multi-select | 非スコープ | **主担当** |

υ での subgraph は、将来 **λ で定義した合成境界**を前提にした UX として進めてよい（本書で API を固定しない）。

---

## 2. Enum（閉集合・型の砦）

### 2.1 目的

閉集合について **ロード時 / 配線時**に誤りを検出し、自由文字列の事故を減らす。ワイヤ上の primitive は引き続き `string` でもよいが、**型グラフ上は `PortSpec.closed_string_variants` により閉集合メタ**を載せる（δ 当初案の **案2** を採用。`SocketType` 自体への `Enum { … }` variant は将来案）。

### 2.2 エンジン（`FlowgraphProgram::build`）

- データエッジについて、上流・下流とも `SocketType::String` のとき、`closed_string_variants` が **両方**ある場合は **上流の集合 ⊆ 下流の集合** を要求する（下流のみ閉集合・上流が無制約 `string` のときは従来どおり許容し、ランタイム検証に委ねる）。
- 代表例: `flowgraph.tts.speak` の `engine` 入力は、登録済み TTS ドライバ名の閉集合を `PortSpec` に載せる。

### 2.3 ローダ（`BuildContext`）

- 下流ポートに `closed_string_variants` があり、上流が **`flowgraph.literal.string`** のとき、プロパティ `value` が閉集合に含まれない場合は **error 診断**とする。
- 上流がリテラル以外の場合、ロード時の値静的解析は行わない（実行時のノード実装が従来どおり検証）。

### 2.4 ユーザ定義 Enum — `[[enums]]`（TOML）

- `FlowgraphFile` に **`[[enums]]`** 配列を追加する（δ 当初の「3 セクションのみ」からの **後方互換拡張**）。未指定は空配列。
- 各要素: `id`（ファイル内一意）、任意 `primitive`（v0 は `string` のみ意味あり）、`variants`（非空文字列の配列）。
- GUI はパース結果の `enums` を **`user_defined` カテゴリ**としてパレットに合成し、各 variant を **`flowgraph.literal.string` のプリセット**として追加可能にする（feature は変えず、既定 `value` のみ差し替え）。

### 2.5 依存解決

型（閉集合）が別定義に依存する場合は、**不足を明示するロードエラー**とする方針（将来、ポートが `enum_id` を参照する拡張時に適用）。

---

## 3. Flowgraph ライブラリ（再利用単位）

### 3.1 境界ノード（v0）

システム提供のマーカー兼スタブ:

| feature | 役割（v0） |
|---------|------------|
| `flowgraph.library.input` | ライブラリの「外部入力」側のスタブ。プロパティ `value` を出力 `value` に出す（単一 string チャネル）。将来: 接続に応じた動的境界ポート。 |
| `flowgraph.library.output` | ライブラリの「外部出力」側のスタブ。入力 `value` を受け取るシンク（下流なし）。将来: 外向き境界の合成。 |

**動的ポート増減**（「接続が定義」）は λ の本丸だが、エンジン・GUI の改修規模が大きいため **λ+** で本実装し、v0 は **単一 data ポート + 上表の挙動**に留める。

### 3.2 ライブラリ判定（v0）

- ファイル内に **`flowgraph.library.input` または `flowgraph.library.output` が 1 つでも存在**すれば「ライブラリ境界を含むフロー」として扱う（メタ必須条件は λ-4 で緩和可能）。
- **ファイル単位スコープ**から開始する。「同一ファイル内の部分宣言のみライブラリ」等は λ+。

### 3.3 `[meta]` 拡張（λ-4）

既存の `title` / `description` / `tags` に加え、任意で次を取りうる:

- `author`, `name`, `version`, `license`, `repos`
- **`library_uses`**: 当ファイルが依存する他フローの fq（`path/to/graph` 形式、`.flowgraph.toml` 接尾辞は正規化で除去）の配列。

**ライブラリ ID**（表示・衝突回避用）の推奨式:

`normalize(author) + "::" + normalize(name) + "::" + normalize(version)`  

ここで `normalize` は前後空白除去 + 非英数字を `_` に落とし、小文字化（詳細は実装 `normalized_library_id` に従う）。いずれか欠ける場合は ID を生成しない（`Option`）。

### 3.4 循環参照

`library_uses` から **ファイル間の有向グラフ**を構築し、**閉路があればロード失敗**とし、診断に閉路に関与する fq を列挙する。未解決の参照はエラー。

### 3.5 複数 Ingress

合成インスタンスが複合 ingress になりうることは許容するが、外部から見たトリガの独立性・順序は既存 engine の意味論と矛盾しないよう、実装フェーズで § を追補する（本書 v0 は宣言のみ）。

---

## 4. δ 仕様との関係

- [`phase-delta-spec.md`](phase-delta-spec.md) §8.6 の `FlowgraphFile` 記述は **λ により拡張**される: `[meta]` / `[[nodes]]` / `[[edges]]` に加え **`[[enums]]` 任意**、および `[meta]` 内の追加フィールドは **未指定なら空 / None** で後方互換を維持する。

---

## 5. 開発サブフェーズ（コミット単位の目安）

| ID | 内容 | 本リポジトリ（v0） |
|----|------|-------------------|
| λ-0 | 本書 + `roadmap.md` + δ / architecture への cross-link | ✅ 完了（`roadmap.md` では Phase λ を **Completed** に配置） |
| λ-1 | `PortSpec.closed_string_variants` + engine 適合 + loader リテラル検証 + TTS `engine` 適用 + GUI 配線 | ✅ 完了 |
| λ-2 | `[[enums]]` パース・診断・パレット `user_defined` | ✅ 完了 |
| λ-3 | `flowgraph.library.input` / `output` 登録 + ドキュメント上の境界意味 | ✅ 完了（v0 スタブ） |
| λ-4 | `FileMeta` 拡張 + `library_uses` 解決 + 閉路検出 | ✅ 完了 |
| λ-5 | CHANGELOG、manual 断片、example 更新 | ✅ 完了 |

Commit メッセージは既存の [Commit Unit Convention](../roadmap.md) に従い `λ-N feat/fix/docs: …` とする（歴史的コミット分割の目安。既に land 済みの変更を後追いコミットする場合も同じ接頭辞を推奨）。

## 7. 実装サマリ（v0 着地内容）

- **案2**採用: `SocketType::String` のまま `PortSpec.closed_string_variants` で閉集合を表現。`FlowgraphProgram::build` とローダが検証し、`flowgraph.tts.speak` の `engine` にレジストリ名を載せる。
- **TOML**: `[[enums]]` と拡張 `[meta]`（`library_uses` 等）。GUI は `user_defined` パレットと保存時シリアライズで追随。
- **境界**: `flowgraph.library.input` / `output` の Pure スタブ。動的ポートは **λ+**。
- **依存**: `load_flowgraph_dir` が `library_uses` の参照先と閉路を検証。

詳細手順は [`manual/flowgraph-enum-and-library.md`](../manual/flowgraph-enum-and-library.md) と [`flowgraph.example/lambda-demo/main.flowgraph.toml`](../../flowgraph.example/lambda-demo/main.flowgraph.toml) を参照。

---

## 6. リスク（短縮）

- δ 当初の「3 セクションのみ」方針からの **スキーマ拡張コスト**（後方互換は「未指定＝空」で吸収）。
- Library 境界の **Exec** セマンティクスと動的ポート（λ+）。
- 合成ノードと **既存 fragment copy/paste** の二重経路の整理（λ+）。
