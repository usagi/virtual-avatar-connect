# Flowgraph: 閉集合 string（Enum）とライブラリ境界（Phase λ）

## 閉集合ポート（`closed_string_variants`）

一部の `string` ポート（例: `flowgraph.tts.speak` の `engine`）は、**登録済みの許容値一覧**を `PortSpec` に載せています。

- **ローダ**: 上流が `flowgraph.literal.string` のとき、プロパティ `value` が閉集合に含まれないと **エラー**になります。
- **エンジン**: 両端のポートに閉集合メタが付いているデータエッジでは、**上流の集合が下流の集合に含まれる**必要があります（下流のみ閉集合のときは従来どおり許容し、実行時検証に委ねます）。
- **GUI**: キャンバス上の型チェックが上記に概ね追従します。

## ユーザ定義 `[[enums]]`

`.flowgraph.toml` に任意で次のような表を追加できます（未指定なら空で後方互換です）。

```toml
[[enums]]
id = "my_mode"
variants = ["a", "b", "c"]
```

- 同一ファイル内で `id` は一意である必要があります。
- GUI のパレットに **`user_defined`** カテゴリが現れ、各 variant を **`flowgraph.literal.string` のプリセット**として追加できます（保存時に `[[enums]]` は元ファイルから引き継がれます）。

## `[meta].mode_groups` / `default_enabled`（RM-3）

Runtime Mode と Flowgraph の対応付け用メタデータ（[`../roadmap/runtime-mode-roadmap.md`](../roadmap/runtime-mode-roadmap.md) §5）。

```toml
[meta]
mode_groups = ["rss", "alerts"]
default_enabled = true
```

- **`mode_groups`**: `conf.toml` の `[modes.*].flowgraph_groups` が参照するグループ名のリスト。空なら「mode による有効化制御の対象外」。
- **`default_enabled`**: 省略時は **`true`**（既存ファイルは従来どおり常時有効扱い）。`false` にすると、mode 未適用環境では既定で inactive 寄りに扱う想定（Mode Manager 実装で解釈）。
- **`mode_groups` に空文字列を含めるとロードエラー**（診断コード `invalid-mode-metadata`）。

ロード結果は `GET /api/v1/control/flowgraph/diagnostics` の JSON に **`file_activation`**（ファイル fq → 上記 2 フィールド）として載る。

### ランタイム（exec 抑止）

`conf.toml` に `[modes.*]` があり、かつ **`default_runtime_mode`** が有効な mode を指しているとき、各ファイルの `mode_groups` と当該 mode の `flowgraph_groups.enable` / `disable` から **そのファイルに属するノードの exec 経路**（初期ソース・外部 trigger・exec 連鎖）が抑止されます。**Pure/Stateful の pull 評価**は止めません（他ファイルからのデータ参照を維持）。

診断 API では **`inactive_exec_nodes`** に抑止中のノード fq ID が列挙されます。`[modes.*]` が空、または `default_runtime_mode` 未設定のときは従来どおり全 exec 許可です。

`[meta].mode_groups` の各名前は、**いずれかの** `[modes.*].flowgraph_groups`（enable または disable）に一度も出てこない場合、ロード時に診断 **`orphan-mode-group`**（警告）が付きます（typo 検出用）。

`PUT/GET` の modes 系 API は、**毎回** `conf.source_path` から `Conf` を再読し、`[modes.*]` の定義と照合する。in-memory なのは **現在選択 mode ID**（`default_runtime_mode` より優先）だけ。

`POST /modes/plan` と `POST /modes/transit`（`dry_run`）で、遷移先宣言に基づく **プレビュー JSON**（`ModeTransitionPlan`）を取得できる。実際にスロットが変わったときは WebSocket `runtime_mode_changed` が飛ぶ。

### RM-2 / RM-5 ノード

| feature | 説明 |
|---------|------|
| `flowgraph.mode.get` | 実効 Runtime Mode ID（Pure）。 |
| `flowgraph.mode.equals` | 指定文字列と実効 ID が一致するか（Pure）。 |
| `flowgraph.mode.transit` | `exec_in` で mode 切替を要求（Effectful）。`State` / `conf_source_path` が無いと `on_reject`。外部 Control trigger は不可。 |

## ライブラリ境界ノード（v0）

| feature | 説明 |
|---------|------|
| `flowgraph.library.input` | 外部入力側スタブ（プロパティ `value` を `value` ポートに出す）。 |
| `flowgraph.library.output` | 外部出力側スタブ（`value` 入力のみ）。 |

動的に境界ポートが増える挙動は **将来フェーズ（λ+）** の対象です。設計の正本は [`../roadmap/phase-lambda-flowgraph-enum-and-library.md`](../roadmap/phase-lambda-flowgraph-enum-and-library.md) を参照してください。

## `[meta].library_uses` と循環検出

ディレクトリロード時、各ファイルの `[meta]` に `library_uses = [ "other/path", ... ]`（fq・拡張子なし）を書くと、**参照先が存在するか**と、**依存グラフに閉路が無いか**が検証されます。閉路や未知参照は **ロードエラー**です。

任意の識別子として `author` / `name` / `version` がすべて揃うとき、ローダは正規化した **ライブラリ ID** 文字列を組み立てられます（API／将来の UI 用）。
