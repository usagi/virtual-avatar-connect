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

## ライブラリ境界ノード（v0）

| feature | 説明 |
|---------|------|
| `flowgraph.library.input` | 外部入力側スタブ（プロパティ `value` を `value` ポートに出す）。 |
| `flowgraph.library.output` | 外部出力側スタブ（`value` 入力のみ）。 |

動的に境界ポートが増える挙動は **将来フェーズ（λ+）** の対象です。設計の正本は [`../roadmap/phase-lambda-flowgraph-enum-and-library.md`](../roadmap/phase-lambda-flowgraph-enum-and-library.md) を参照してください。

## `[meta].library_uses` と循環検出

ディレクトリロード時、各ファイルの `[meta]` に `library_uses = [ "other/path", ... ]`（fq・拡張子なし）を書くと、**参照先が存在するか**と、**依存グラフに閉路が無いか**が検証されます。閉路や未知参照は **ロードエラー**です。

任意の識別子として `author` / `name` / `version` がすべて揃うとき、ローダは正規化した **ライブラリ ID** 文字列を組み立てられます（API／将来の UI 用）。
