# Tutorial: Dictionary Editor & Live Quick-Add (Phase φ)

配信中の「誤読だったので今すぐ辞書に追加したい」「過去の学習を取り消したい」を **GUI だけで完結**させるためのチュートリアル。

> 関連ソース:
> - Control API: [`src/web_interface/control/table/mod.rs`](../../../src/web_interface/control/table/mod.rs) /
>   [`src/web_interface/control/flowgraph/mod.rs`](../../../src/web_interface/control/flowgraph/mod.rs)
> - GUI: [`gui/src/lib/control/dictionary/`](../../../gui/src/lib/control/dictionary/)
> - ロードマップ仕様: [phase-phi-control-api-dictionary-editor.md](../../roadmap/phase-phi-control-api-dictionary-editor.md)

---

## ねらい

- **Dictionary Editor Pane**（11 カラム一覧 / sort / filter / 編集 / 削除 / 409 衝突解決）で TSV 辞書を直接編集する
- **Live Quick-Add Widget** で `dictionary.learn` / `dictionary.forget` に 1-shot trigger を発火する
- `conf.toml` の `[[control_api.tables]]` + `quick_add` をどう書くと GUI から見えるようになるかを押さえる
- `control_triggerable` opt-in の範囲を知る（任意ノードの暴発トリガは不可）

---

## 全体像

```
┌─ GUI (Live タブ) ───────────────────────────────────────────┐
│  Live Quick-Add  ─ source/replacement/[Learn]/[Undo]        │
│        │                                                    │
│        │ POST /api/v1/control/flowgraph/default/trigger/..  │
│        ▼                                                    │
│   Control API  ──[write TSV]──▶  dictionary.*.tsv           │
│        ▲                                                    │
│        │ GET /api/v1/control/table/{key}（If-Match 付き）   │
│  Dictionary Editor Pane                                     │
└──────────────────────────────────────────────────────────────┘
                         │
                         │ flowgraph.table.load_tsv が次の exec_in で再読み
                         ▼
        flowgraph.dictionary.replace / .match  → TTS など下流へ
```

- **Editor** は TSV ファイルを直接読み書きする。`If-Match: b3:<hash>` で楽観ロック。
- **Quick-Add** は Flowgraph Trigger API 経由で `dictionary.learn` ノードを発火する。
  - ノード側は `control_triggerable = true` を opt-in している必要がある。
  - 学習結果が TSV に永続化されるかどうかは **flowgraph 側の配線**次第
    （`learn → table.write_tsv` を張っていれば永続化、張っていなければメモリ限定）。

---

## 前提

- [`flowgraph.example/dictionary/basic-replace.flowgraph.toml`](../../../flowgraph.example/dictionary/basic-replace.flowgraph.toml)
  などの辞書を食わせる flowgraph が既に動いている
- GUI （Svelte ビルド済み）が `gui/dist/` に配置済み、または `npm run dev` で別ポート経由でアクセスしている

---

## Step 1 — `conf.toml` に Table を登録

```toml
# [control_api] セクションは既存。allow-list をここに追加する。
[[control_api.tables]]
key      = "chat_dict"                       # URL / localStorage キー
path     = "dictionary.chat.dict.tsv"        # ディスク上の TSV
label    = "Chat 辞書"
role     = "dictionary"                      # これが無いと Editor の tab に出ない
editable = true

[control_api.tables.quick_add]
node_id        = "chat-echo/main::learn"     # dictionary.learn の fq ID
kind           = "literal"                   # "literal" | "regex"
forget_node_id = "chat-echo/main::forget"    # 未指定なら [Undo] は無効化
```

`fq ID` は `{フォルダ}/{ファイル名}::{ノード ID}` 形式（拡張子 `.flowgraph.toml` は抜く）。
GUI の Flowgraph タブ → Node Card の「id」欄や、`GET /api/v1/control/flowgraph/tree` でも確認できる。

> ※ `[[control_api.tables]]` は **起動時 snapshot**。反映には VAC の再起動が必要（reload では拾わない）。

---

## Step 2 — 対応する Flowgraph を配線

最低限 `dictionary.learn` と（Undo が欲しければ）`dictionary.forget` を置く。

```toml
# 例: flowgraph/chat-echo/main.flowgraph.toml の抜粋
[[nodes]]
id = "dict_path"
feature = "flowgraph.literal.string"
properties.value = "dictionary.chat.dict.tsv"

[[nodes]]
id = "load"
feature = "flowgraph.table.load_tsv"

[[nodes]]
id = "learn"
feature = "flowgraph.dictionary.learn"

[[nodes]]
id = "forget"
feature = "flowgraph.dictionary.forget"

# 学習結果を TSV に書き戻す（Quick-Add で学んだ内容を永続化したいなら必要）
[[nodes]]
id = "write"
feature = "flowgraph.table.write_tsv"

[[edges]]
from = "dict_path:value"
to   = "load:path"

# Quick-Add から来た exec が learn → write_tsv へ
[[edges]]
from = "learn:updated_dictionary"
to   = "write:table"
[[edges]]
from = "dict_path:value"
to   = "write:path"
[[edges]]
from = "learn:on_learned"
to   = "write:exec_in"

# forget も同様
[[edges]]
from = "forget:updated_dictionary"
to   = "write:table"
[[edges]]
from = "forget:on_forgotten"
to   = "write:exec_in"
```

配線しない場合、Quick-Add の Learn は**メモリ上の Table**にだけ乗り、
VAC を再起動すると消える（デモや一時的な言い換えには使える挙動）。

---

## Step 3 — GUI から編集する

VAC を再起動して Live タブを開く。以下 2 つのウィジェットが現れる。

### Dictionary Editor Pane（11 カラム一覧）

- 列: `source / replacement / kind / priority / is_locked / enabled / by / created_at / expires_at / tags / note`
- ヘッダクリックでソート方向トグル
- 検索ボックスは `source / replacement / tags / note / by` を部分一致
- `kind` / `disabled を隠す` / `expired を隠す` のフィルタ付き
- 行右端の **[編集]** でダイアログが開く（全カラムを編集可能、`Ctrl+Enter` で送信）
- **[削除]** は `is_locked = true` の行では disabled
- 他クライアントと編集が衝突すると **409 → 3-way 解決ダイアログ** が自動で開く
  - `base`（編集開始時）/ `mine`（送信値）/ `server`（最新）を並べて見せる
  - 「自分の編集を強制」「サーバ現行を採用」「フィールドごとに merge」の 3 パスから選ぶ

### Live Quick-Add Widget（1 行フォーム）

- `source` → `replacement` を入力して **[Learn]**（`Ctrl+Enter` でも可）
- 履歴は per-table で直近 10 件、各行に **[Undo]**
  - `forget_node_id` が conf に未設定なら自動的に disabled
  - Locked な行は `forget` ノード側で保護されるため Undo しても何も起きない（toast に「Removed=0 / Locked=1」相当が返る）

送信先は:

```
POST /api/v1/control/flowgraph/default/trigger/chat-echo/main::learn
Content-Type: application/json
Authorization: Bearer <token>

{
  "inputs": {
    "source": "ドクターウサギ",
    "replacement": "どくたーうさぎ",
    "kind": "literal",
    "by": "gui:quick_add"
  }
}
```

---

## よくある落とし穴

| 症状 | 原因 | 対処 |
|---|---|---|
| Editor のタブに何も出ない | `role = "dictionary"` 未指定 | conf の該当エントリに `role` を追加して再起動 |
| Quick-Add が出ない | `quick_add` ブロック未指定 | `[control_api.tables.quick_add]` を書き、`node_id` を fq ID で指定 |
| Trigger 時に 400 `control_triggerable_forbidden` | 発火先が opt-in していないノード | `node_id` が typo、または learn/forget 以外の feature を指している。Phase φ-2 時点で opt-in しているのは `dictionary.learn` / `.forget` のみ |
| Trigger 時に 404 `instance_not_found` | V2 は単一 instance のため `instance_id = "default"` のみ有効 | GUI は自動で `default` を使うので、手動 curl の場合のみ注意 |
| Learn しても Editor に反映されない | `learn → table.write_tsv` の配線が無く、TSV にまだ書かれていない | 永続化したいなら Step 2 のように `write_tsv` を繋ぐ。もしくは Editor の「↻ Reload」を押す |
| 409 が連続する | 他ユーザ（別タブ）と同時編集中 | 3-way 解決ダイアログで採用方針を決める。衝突が辛ければ `editable = false` で片側を read-only 化 |
| Quick-Add 履歴が消えた | `localStorage` をクリアした | 仕様。サーバ側履歴は持たない（`vac.dictQuickAdd.history.<key>`） |

---

## 安全設計のポイント

- **allow-list 外のファイルは 404**（存在隠蔽）。Control API のパストラバーサル攻撃に対する一次防衛。
- **is_locked = true の行は PATCH / DELETE 不可（403）**。`forget` ノード側も `on_locked` 経路に出す。
- **Trigger API は `control_triggerable = true` のノードに限定**（400 で拒否）。任意ノード乗っ取り防止。
- **楽観ロックのみ**で悲観ロックは無い。UI が `If-Match` を送ることで race window を detect 可能にしている。

---

## 関連

- [conf-reference.md §5.1](../conf-reference.md#51-control_apitables--辞書--汎用-table-の-gui-編集許可リスト-phase-φ)
- [Node Catalog](../node-catalog.md) の `flowgraph.dictionary.*` / `flowgraph.table.*`
- Roadmap: [phase-phi-control-api-dictionary-editor.md](../../roadmap/phase-phi-control-api-dictionary-editor.md)
- Backlog: [backlog-nodes.md](../../roadmap/backlog-nodes.md)（`flowgraph.util.timer_interval` 予定など）
