# Glossary Rename Roadmap

現行 VAC 専用の `Dictionary` 機能を `Glossary` へ改名し、`Dictionary` を将来の汎用 key-value / map 型に譲るための破壊変更計画。
目的は、データ構造の `Dictionary` と Iterator / Ranges 操作の `.map` を共存させ、Flowgraph の言語感を長期的に濁らせないこと。

## 1. 結論

- VAC 専用の語彙・読み・表記ゆれ・正規化・コマンド語彙表: `Glossary`
- 汎用 key-value データ構造: `Dictionary`
- Iterator / Ranges 変換操作: `.map`
- UI 表記: `Glossary（用語集）`

`Lexicon` は意味としては最良だが、日本語ユーザーへの通りやすさを優先して `Glossary` を採用する。

## 2. 対象

### 改名するもの

- `flowgraph.dictionary.replace` -> `flowgraph.glossary.replace`
- `flowgraph.dictionary.match` -> `flowgraph.glossary.match`
- `flowgraph.dictionary.learn` -> `flowgraph.glossary.learn`
- `flowgraph.dictionary.forget` -> `flowgraph.glossary.forget`
- GUI `Dictionary Editor` -> `Glossary Editor`
- GUI `Live Quick-Add` の dictionary 文言 -> glossary 文言
- `role = "dictionary"` -> `role = "glossary"`
- docs / manual / tutorial / E2E spec の用語

### すぐには変えないもの

- `.dict.tsv` ファイル拡張子は互換 alias として当面残す
- 既存 `dictionary.*.txt` / `dictionary.*.dict.tsv` は migration 対象として読む
- 内部ソースファイル名は段階移行可。初回 PR で一気に rename できるなら行う

## 3. 互換方針

v2 中は破壊変更を許容するが、ユーザーの既存サンプルを壊しすぎないため alias を置く。

- `flowgraph.dictionary.*` は deprecated alias として 1 段階残す
- alias 使用時は load diagnostics に warning を出す
- `role = "dictionary"` は `role = "glossary"` と同等に扱い、GUI では glossary と表示する
- manual は `glossary` を正、`dictionary` を旧称として扱う
- 互換削除タイミングは v2 安定化後に判断する

## 4. 実装順序

### GRN-1 docs / terminology

計画と用語を固定する。`Dictionary` は汎用 key-value、`Glossary` は VAC 専用語彙表として定義する。

### GRN-2 flowgraph node alias

`flowgraph.glossary.*` を正規 feature 名として追加し、既存 `flowgraph.dictionary.*` は deprecated alias にする。

- registry は両方を受け付ける
- catalog は `glossary.*` だけを通常表示する
- alias は diagnostics で旧称 warning

### GRN-3 control API / config role

Control API table catalog の role を `glossary` に移行する。

- `role = "glossary"` を正とする
- `role = "dictionary"` は互換 alias
- Quick-Add node id の説明を `glossary.learn` / `glossary.forget` に更新する

### GRN-4 GUI rename

GUI 表示名とコンポーネント名を移行する。

- `DictionaryEditorPane` -> `GlossaryEditorPane`
- `DictionaryTable` -> `GlossaryTable`
- `DictionaryEntryForm` -> `GlossaryEntryForm`
- `DictionaryConflictDialog` -> `GlossaryConflictDialog`
- stores / tests / labels を glossary に揃える

### GRN-5 examples / tests / migration

サンプル Flowgraph、E2E fixture、manual を更新する。

- 新規 sample は `glossary.*` を使う
- 旧 sample は migration note へ移す
- E2E spec 名も glossary へ移す
- `v1-to-v2-migration.md` / `conf-reference.md` を更新する

### GRN-6 Dictionary as generic key-value

Glossary 移行後に、汎用 `dictionary<K,V>` / `flowgraph.dictionary.*` を標準ライブラリー設計へ戻す。
この段階までは `dictionary` 名前空間を新規用途に使わない。

## 5. Map / Dictionary / .map の整理

- `map<K,V>` は内部型名として残してよいが、ユーザー向け表示は `Dictionary` を優先する
- `.map` は Iterator / Ranges の変換操作として維持する
- GUI では `Map` データ構造を前面に出さず、`Dictionary` / `Key-Value` と説明する
- `table.map_rows` は `table.map_rows` のまま。ただし UI 表示は「行を変換」でもよい

