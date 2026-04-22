# v2 → main マージ PR 本文ドラフト

`v2` ブランチを `main` にマージするときの PR テンプレート。実際の PR では本文を
そのままコピーして使える粒度にまとめてある。

---

## Summary

Flowgraph-only アーキテクチャへの最終移行（v0.10.0）。v1 の `[[processors]]` 層は完全に
撤去され、ingress〜変換〜出口のすべてが `flowgraph_dir` 配下の `.flowgraph.toml` と
Flowgraph Runtime で表現される。直近の v2 マージ準備ロードマップ（ζ-3 → γ-4a.0 → γ-4a →
γ-2 → δ-X → ζ-4）で整地を済ませた。

主な内容:

- **ζ-3: Flowgraph reload 時のブリッジ再配線**
  `BridgeHandles` を `SharedState` に常時保持し、`reload_runtime` から旧 bridges と旧
  `FlowgraphRuntime` worker を graceful shutdown してから一括 respawn する経路を統一。
  `web_input` endpoint 差分時は `ControlEvent::RestartRecommended` を push して GUI に
  リスタート推奨 toast を出す。
- **γ-4a.0: GUI ノード/エッジ削除 UX**
  ノードカード hover 時の × ボタン、`ondelete` で一括削除、toast から「元に戻す」。
- **γ-4a: Pipeline エディタ基盤**
  `isDirty` 追跡、`Save *` 表示、Ctrl+S 保存、未保存 `beforeunload` ガード。
- **γ-2: Live タブ仕上げ**
  `POST /managed_apps/:id/restart`、`ManagedAppDrawer` の再起動ボタン、`BosPreview` の
  縦画面時アコーディオン。
- **δ-X: `flowgraph.command.set` ノード**
  V1 scene switcher の Flowgraph ネイティブ版。`flowgraph.example/command-sets/` サンプル。
- **ζ-4: AI-Twitch ハイブリッド方針**
  `conf.example-openai-chat.toml` に `vac_twitch_chat_say` 利用基準を行動規範として記述。
- **η: Dictionary/Table Unification**（v2 上で継続実装、main へのマージは **この PR には含めない**）
  - V1 時代に合意していた 11 カラム辞書仕様と runtime 学習/忘却を V2 Flowgraph 上に再構築。
  - `SocketType::Table` を追加、`flowgraph.table.*` 汎用ノード 4 種と `flowgraph.dictionary.*` 4 種
    （Replace / Match / Learn / Forget、前 2 者は Stateful で AC + Regex キャッシュ）を導入。
  - `flowgraph.dictionary.command`（V1 由来の固定文法）を削除し、`dictionary.match` +
    `commands.tsv` + `dictionary.learn` / `.forget` による「文法もユーザーが編集可能」な方式に置換。
  - V1 `dictionary.*.txt` / `regex.*.txt` / `regex.*.csv` を 11 カラム TSV に変換する
    `virtual-avatar-connect-migrate-dict` CLI を新設、既存ファイル 6 種を実際に migrate 済み。
  - GUI は Table ポートの視覚区別のみ実装。11 カラム編集 UI と Quick-Add Widget は Control API
    未整備のため φ 以降に保留。
  - 仕様書: [`docs/roadmap/phase-eta-dictionary-unification.md`](./phase-eta-dictionary-unification.md)
  - **本 PR のスコープ外**: η 系コミットは別ブランチ（あるいは `v2` 上の後続コミット群）に
    分割 PR として出し、main には先にこの v2 merge PR を入れてから順次 cherry-pick / merge する想定。
- **merge prep**: `Cargo.toml` を 0.10.0 に bump、`CHANGELOG.md` 更新、
  `docs/manual/v1-to-v2-migration.md` 追加、README に移行ガイド導線を追加。

## Breaking changes

- `[[processors]]` は TOML には残ってもランタイムでは参照されない（起動時警告）。
- `channel_to` は非推奨。Flowgraph の `channel.emit` へ移行。
- `PauseTarget::Processors` / V1 Control API（`/processors/:i/config` 等）はすでに v0.9.x
  で削除済み。再追加はしない。

移行ガイド: [`docs/manual/v1-to-v2-migration.md`](../manual/v1-to-v2-migration.md)

## Test plan

- [x] `cargo test --lib`（全 358 passed）
- [x] `cargo check --lib`（warn 2 のみ、v1 残滓の未使用関数で想定内）
- [x] `cd gui && npm run check`（svelte-check 0 errors / 0 warnings）
- [x] `cd gui && npm run build`（vite build 成功）
- [x] `flowgraph.example/` が `load_flowgraph_dir` でエラー無くロードされる
- [x] `BLESS_NODE_CATALOG=1` で再生成した `docs/manual/node-catalog.md` が最新
- [ ] `cargo build --release`（レビュー担当側で確認）
- [ ] `cargo run -- conf.example-*.toml` で各 example の deserialize が通る（レビュー担当）
- [ ] `conf.local.solo.toml` 等の実運用 conf で 1 配信分の回帰確認（メンテナ）

## 既知の回帰

- `windows` crate 0.61 → 0.62 の API 失効（OCR / screenshot 系）は v0.9.x 時点で
  `anyhow::bail!` に暫定 fallback 済み。δ-9.3 で `windows-future` 経由に再実装予定。

## η フェーズの分割 PR 戦略（main 合流後の段取り）

`η` 実装は v2 ブランチ上で継続しているが、本 merge PR には含めない方針。main 合流後に
以下の単位で順次 PR を出す（いずれも v2 ベースで作った feature branch を main に向ける）:

1. **η-PR1（型と汎用 Table ノード）**
   η-0（仕様書）+ η-1a（`SocketType::Table`）+ η-1b（`src/flowgraph/table.rs`）+ η-2（`flowgraph.table.*` 4 ノード）。
   影響範囲: `src/flowgraph/socket.rs` / `src/flowgraph/table.rs` / `src/flowgraph/nodes/table_ops.rs`、
   `Cargo.toml` に `blake3` / `aho-corasick` 追加、`docs/roadmap/phase-eta-dictionary-unification.md`。
2. **η-PR2（辞書ノードと example）**
   η-3（`flowgraph.dictionary.replace/match/learn/forget` 4 ノード刷新）+ η-4（`dictionary.command` 削除、
   `flowgraph.example/dictionary/` 新設、`phase-delta-spec.md` §2.1 / §10.2 更新、CHANGELOG Breaking 追記）。
   影響範囲: `src/flowgraph/nodes/dictionary.rs` 全面書き換え、`src/flowgraph/registry.rs` 登録更新、
   `flowgraph.example/chat-filters/main.flowgraph.toml` コメント更新、`docs/manual/node-catalog.md` 再生成。
3. **η-PR3（migrate CLI）**
   η-5（`src/bin/migrate_dict.rs` 新設、`Cargo.toml` に 2 つめの `[[bin]]` 追加、既存 V1 ファイルの
   `.dict.tsv` 変換済み artifact をリポジトリに追加、仕様書 §8.1 更新）。
4. **η-PR4（GUI Table ポート視覚区別）**
   η-6 のうち実装済み分（`FlowgraphNodeCard.svelte` の `handleClass` / CSS 更新）。
   Dictionary Editor pane / Live Quick-Add は φ 以降別 PR。

この 4 本は論理的に η-PR1 → η-PR2 → η-PR3 → η-PR4 の順で依存する（PR2 は PR1 の型を使い、
PR3 は出力フォーマットが PR1 / PR2 と揃っている必要があり、PR4 は PR1 の Table 型が前提）。
いずれも `v2` ブランチ上に既に commit 済みなので、main 合流後は `git cherry-pick` か
`git log --reverse` からの個別 PR 作成でよい。

## 関連

- ロードマップ原本: `.cursor/plans/v2_merge-ready_roadmap_*.plan.md`
- η 計画: `.cursor/plans/eta-dictionary-unification_*.plan.md`
- η 仕様書: [`phase-eta-dictionary-unification.md`](./phase-eta-dictionary-unification.md)
- CHANGELOG: [`CHANGELOG.md`](../../CHANGELOG.md)
- ノード一覧: [`docs/manual/node-catalog.md`](../manual/node-catalog.md)
