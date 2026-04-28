# Flowgraph Standard Library Roadmap

VAC Flowgraph を汎用言語へ近づけるための標準ライブラリー計画。
目的は C++ の STL を再実装することではなく、常駐データフロー、配信支援、シミュレーション、抽選、イベント処理で実用になる Ranges / LINQ / Iterator 的なコレクション処理、データ構造、アルゴリズムを、Flowgraph から安全に使える形で提供すること。

## 1. 方針

- **小さく純粋に始める**: 乱数、分布、sort、search、データ構造は Pure / Stateful の境界を明確にする。
- **seed と再現性を重視する**: 配信企画、テスト、シミュレーションで同じ結果を再現できるようにする。
- **巨大計算を許さない**: max items / timeout / memory budget を持たせ、常駐ランタイムを詰まらせない。
- **GUI で選べる粒度にする**: 教科書的に正しいだけでなく、ユーザーが選びやすい名前とプリセットを用意する。
- **Rust crate に寄せる**: PRNG / distribution / data structure は信頼できる crate を使い、Flowgraph 側は型・診断・capability の wrapper に集中する。
- **Iterator 風パイプラインを中核にする**: list / table / stream を map / filter / take / window / aggregate でつなぎ、無制限 loop より先に有限で観測可能な処理を整える。

## 2. 実装順序

### SL-0 Ranges / LINQ / Iterator foundation

標準ライブラリーの中核。C++ Ranges / C# LINQ / Rust Iterator の発想を Flowgraph 向けに落とし込む。

- `flowgraph.list.map`, `flowgraph.list.filter`, `flowgraph.list.flat_map`
- `flowgraph.list.take`, `flowgraph.list.skip`, `flowgraph.list.chunk`, `flowgraph.list.window`
- `flowgraph.list.fold`, `flowgraph.list.reduce`, `flowgraph.list.scan`
- `flowgraph.table.select`, `flowgraph.table.filter`, `flowgraph.table.map_rows`
- `flowgraph.table.join`, `flowgraph.table.group_by`, `flowgraph.table.aggregate`
- `flowgraph.stream.debounce`, `flowgraph.stream.throttle`, `flowgraph.stream.window`

Stage 3 Collection Processing と接続する。callback graph は最初 Pure only とし、Effectful callback は bounded exec loop として別フェーズに分離する。

### SL-1 PRNG foundation

乱数の基礎。最初に seedable generator と状態の扱いを固める。

- PRNG candidates: LCG, PCG, xoshiro256++, xoshiro256**, Philox, ChaCha8, ChaCha20, SplitMix64
- `SplitMix64` は主に seed expansion 用
- pure mode: `seed` + `index` から値を決定する stateless generator
- stateful mode: generator state を持ち `next` exec で進める node
- cryptographic 用途は ChaCha20 を置けるが、秘密生成・鍵用途には使わせない診断を出す

### SL-2 Distribution library

PRNG にアダプトできる分布群。最初は `rng` / `seed` / `params` / `count` を共通化する。

- continuous: uniform, normal, lognormal, exponential, gamma, beta, Pareto, Weibull, Rayleigh, Cauchy, triangular
- discrete: discrete uniform, Bernoulli, binomial, geometric, Poisson, categorical, Dirichlet
- output: scalar と list の両方
- invalid parameter は `result<T>` または `on_error` で診断する
- Table draw の weight sampling と categorical distribution は同じ内部実装を共有する

### SL-3 Sequence / buffer data structures

常駐イベント処理で実用頻度が高い構造から入れる。

- ring buffer
- queue / deque
- fixed-size sliding window
- reservoir sampling buffer
- moving aggregate buffer

`StatefulNode` と persistent graph state の境界が重要。最初は worker lifetime state として実装し、restart を跨ぐ永続化は別 subphase。

### SL-4 Sort / search algorithms

Table / list 操作の標準部品。教科書的な網羅より、実用上の選択肢を優先する。

- stable sort / unstable sort
- bucket sort
- partial top-k
- binary search
- lower_bound / upper_bound
- skip list index

quicksort や red-black tree は内部実装としては有用だが、Flowgraph のユーザー向け API として露出する優先度は低い。ユーザーには「stable sort」「top-k」「indexed lookup」のような目的語で見せる。

### SL-5 Indexed collections

検索・近似・ランキング用。GUI から扱える形にするには schema と key column の設計が必要。

- skip list
- ordered map
- bucket index
- hash index
- priority queue

Table の index として使うか、独立した collection 型として使うかは `record` / `table schema` が固まった後に確定する。

## 3. 型と責務

- `bytes`: MsgPack / binary file / PRNG state serialization の基礎
- `result<T>`: distribution parameter error、parse error、DB error の基礎
- `record`: algorithm input / output の named schema
- `dictionary<K,V>`: 汎用 key-value データ構造。VAC 専用語彙表は `Glossary` へ移行して名前空間を空ける
- `table`: sort / filter / draw / index の主対象
- future `collection<T>`: ring buffer や index を第一級値として扱う場合の候補

## 4. GUI 表示方針

Catalog ではアルゴリズム名を前面に出しすぎない。

- Random / PRNG
- Random / Distribution
- List / Query
- Table / Query
- Table / Sort
- Table / Draw
- Stream / Window
- Collection / Buffer
- Collection / Index

高度な実装名は Advanced 設定に隠す。通常ユーザーには「絞り込み」「列を選ぶ」「直近 N 件」「グループ集計」「正規分布」「重み付き抽選」「上位 K 件」のような目的で見せる。
