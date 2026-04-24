# Phase ν — GUI E2E testing with Playwright

> **Status**: ν / ν-β ともに **完了**。初期 5 ケース全 5 通し（§3.1 control-panel-smoke /
> §3.2 flowgraph-canvas-basic / §3.3 dictionary-editor-409-merge / §3.4
> live-quick-add-learn-undo / §3.5 channels-ws-live-update）、engine 側の
> `Table::empty()` default coerce 修正 (ν-β-3) も着地済み。ローカル `npx playwright
> test` が **5 specs / 1 worker / ~9 s** で全緑。
> 起点となるスコープ感は [`../roadmap.md`](../roadmap.md) の "Phase ν" / "Phase ν-β" を参照。

---

## 0. Status

- 現状: `gui/` に自動テストは 0 件（`svelte-check` の型検査 / `eslint` のみ）。
- 動機は §1。スコープ・アーキ判断は §2〜§5、サブフェーズ分解は §6 に固定する。
- ν-0 の成果物は **本書そのもの** + 次の合意:
  - fixture conf を **新規の `conf.fixture.e2e.toml`** として配置する（conf.local.toml 系とは独立）。
  - Playwright は **gui/** 配下に閉じる（Rust 側には入れない）。
  - CI 化は optional（ν-2 以降、まずはローカル開発者ループを確立する）。

---

## 1. 動機（E2E でしか拾えない挙動）

型チェックと unit test では届かない層が φ / γ 系の積み上げで急拡大している。

| 領域 | 代表的な壊しやすい箇所 |
|---|---|
| Phase φ: Dictionary Editor Pane | optimistic lock / 409 → 3-way merge ダイアログ / Live Quick-Add の Learn + Undo |
| γ-4a / γ-4a.0: Flowgraph Canvas | ノード追加 → 接続 → 削除 / Save-dirty 表示 / Ctrl+S / beforeunload ガード / Property Editor の editor_order |
| γ-2: Live Tab / Managed App Drawer | restart コマンド投入 → status polling で UI 反映 |
| γ-X: WebSocket 系 | `/ws/control` subscribe → ingress で channels が逐次更新 |
| OBS Browser Source | `/browser-output/*` の WebKit 挙動（TTS / pause 統合の既知 gotcha） |

既存 unit test ではどれも「裏の API / ストアだけ触れる」ので、**UI レイヤ + WS + HTTP + VAC 本体** まで通しての回帰はスモーク以外で検知できない。

---

## 2. スコープ方針

**narrow-scoped** にする。「Playwright を導入してレギュラに回せる形にする」までが ν のゴールで、全 UI の網羅は目標ではない。

- 初期 5 ケース（§3）で "フィクスチャが機能していて CI 的にも安定" なことを示す。
- `playwright install` / `npx playwright test` / `npx playwright codegen` のローカル DX が通っていれば十分。
- visual regression（`toHaveScreenshot`）、MSW 的モック、多ブラウザマトリクス、Docker 上での `webServer` 起動などは **ν+**（将来フェーズ）に送る。

以下は非スコープ:

- **Rust 側の integration test 拡張**（`tokio::test` + `actix-web` の test server）— 別フェーズ扱い。
- **Storybook 的 UI 単体テスト**（Playwright Component Testing）— ν+。
- **Twitch / OpenAI / TTS などの外部 IO のモッキング** — ν-1 の fixture conf で「最初から全部 disabled」にすることで回避する。

---

## 3. 初期 5 ケース仕様

以降 `§3.N` = `tests/e2e/N.spec.ts` 相当と捉えてよい。すべて `conf.fixture.e2e.toml` を `webServer` で起動し、Bearer token は fixture 内の固定値を使う。

### 3.1 `control-panel-smoke.spec.ts`

- 目的: 起動が通り、認証が疎通し、最小 UI が描画されることを確認する。
- シナリオ:
  1. `baseURL/gui/?token=<fixture token>` に navigate。
  2. `TabNav`（Live / Setup / Flowgraph / Logs / Tools）が描画される。
  3. `/api/v1/control/channels` が 200 を返す（HTTP 直叩きで認証を確認）。
- 失敗条件: 500 / 401 / Tab が表示されない / `ToastLayer` に fatal トーストが残る。

### 3.2 `flowgraph-canvas-basic.spec.ts` — **完了 (ν-β-2)**

- 目的: γ-4a の dirty tracking + Ctrl+S + fixture 往復の生存確認。
- 実装:
  1. 事前に `GET /api/v1/control/flowgraph/file/sample` で TOML を snapshot。
  2. Flowgraph タブ → ファイルツリーの `sample` を開き、`.svelte-flow` + `#in` /
     `#log` の可視を確認。
  3. Save ボタン label が `Save`（clean）であることを確認。
  4. Palette の検索入力に `util.log` を入れ、`flowgraph.util.log` のみに絞ってから
     クリック → `log_2` が追加される（FlowgraphPalette は dblclick 的 click-add で、
     HTML5 DnD 経路は使わない）。
  5. Save ボタン label が `Save *`（dirty）へ遷移。
  6. `Ctrl+S` で `PUT /api/v1/control/flowgraph/file/sample` 200 → request body に
     `id = "log_2"` と `flowgraph.util.log` が含まれることを確認。
  7. Save ボタンが `Save` に戻る + 再 GET で `log_2` が永続化されている。
  8. `finally` で snapshot TOML を `PUT` し直し、fixture を汚さない。
- 非スコープ:
  - Svelte Flow 内の **handle drag による edge 接続** と **`beforeunload` ガードの
    発火確認**。前者は pointer event 合成と `getViewport()` の初期アニメ絡みで
    Playwright からは flake りやすく、後者は Chromium が `runBeforeUnload=true`
    の場合だけ dialog を投げる仕様のため、ν-β のスコープからは除外。ν+（将来
    フェーズ）で扱う。
- spec: `gui/tests/e2e/flowgraph-canvas-basic.spec.ts`

### 3.3 `dictionary-editor-409-merge.spec.ts` — **完了 (ν-β-1)**

- 目的: φ-3d の 3-way merge ダイアログが **実際の 409** で機能すること。
- 実装:
  1. Preflight: `GET /api/v1/control/table/sample_dict` で現在値を取り、
     `Dr.USAGI` 行の replacement が `ドクターウサギ` でなければ先に戻す（中断した
     前回 run の自己治癒）。
  2. Live タブ（既定表示）の `Dictionary Editor` を `<h3>` + `xpath=ancestor::section[1]`
     で掴み、検索入力で `Dr.USAGI` 行に絞って [編集] を開く。
  3. 編集フォーム内で `replacement` を `MINE-<nonce>` に書き換える。
  4. **別クライアント (Playwright `request`)** で同じ row を `PATCH` し、
     `If-Match: b3:<current_hash>` で `SERVER-<nonce>` を書き込んで server 側 hash を進める。
  5. GUI で「更新する」→ `PATCH` が **409 `optimistic_lock_failed`** で返ることを
     `waitForResponse` で確認。編集フォームは閉じる。
  6. `DictionaryConflictDialog`（aria-label `/辞書編集で競合/`）が開き、3-way 表に
     `MINE-<nonce>` / `SERVER-<nonce>` の両方と `replacement` カラム名が並ぶこと。
  7. 「自分の編集を強制」→ fresh hash で `PATCH` 200 → ダイアログが閉じる。
  8. `GET /api/v1/control/table/sample_dict` で `replacement === MINE-<nonce>` を最終確認。
  9. Cleanup: replacement を `ドクターウサギ` に `PATCH` で戻し、`sample.dict.tsv`
     に差分を残さない（line-ending は `.gitattributes` で `eol=lf` 固定済み）。
- spec: `gui/tests/e2e/dictionary-editor-409-merge.spec.ts`
- 実装メモ:
  - API 上 `revision` 番号は存在せず、`If-Match: b3:<content_hash>` による
    optimistic lock が実体。409 レスポンス body は diff 情報を持たないので
    GUI は再 GET で 3-way を構築する。
  - `data-testid` を増やさず、`getByLabel(/^replacement/)` と role dialog 名で
    すべてのセレクタを accessible name 経路に寄せた（§5.3）。

### 3.4 `live-quick-add-learn-undo.spec.ts` — **完了 (ν-2.4)**

- 目的: φ-4 の Live Quick-Add → Learn → Undo の往復を、trigger API の発火と
  `DictionaryLiveQuickAdd` の UX 側から両面検証する。
- シナリオ:
  1. `/gui/` ロード後、`GET /api/v1/control/tables` で `sample_dict`
     (quick_add 指定あり) が返ることを事前確認。
  2. Live Quick-Add widget の source / replacement 入力を埋めて [Learn] クリック。
  3. `page.waitForRequest` で `POST /flowgraph/default/trigger/sample::learn`
     を捕捉し、body (`source`/`replacement`/`kind=literal`/`by=gui:quick_add`) と
     レスポンス 202 Accepted を検証。
  4. 履歴カウンタが `(1)` に変わり、展開して [Undo] を押すと
     `POST /flowgraph/default/trigger/sample::forget` が `mode=latest` で飛ぶ。
  5. 行の label が "Undone" に遷移。
- 実装メモ: fixture は UI レベル検証に限定し、`table.write_tsv` 配線 (= ファイル
  永続化) までは作らない。Table 型入力の engine 側 `with_default` が現状
  `MissingRequiredInput` を吐くため、`learn:dictionary` / `forget:dictionary` には
  `flowgraph.table.from_json` の Pure 出力を噛ませて評価を通す (§8 risk 追補)。

### 3.5 `channels-ws-live-update.spec.ts` — **完了 (ν-2.5)**

- 目的: `/api/v1/control/events` → Live タブの channel view リアルタイム反映を WS 経由で検証。
- 実装: `page.waitForEvent('websocket')` で `/api/v1/control/events` への接続を掴み、
  `POST /api/v1/control/ingress` で投入したユニーク content が `channel_datum`
  フレームとして同じ session に配信されることを `framereceived` で確認。
- 失敗条件（実装時の観察メモ）: `ControlEvent::ChannelDatum` は flat struct
  (`channel`/`content` が top-level)。初期実装で `obj.datum.channel` のように
  nested 想定の predicate を書いたため timeout していた — 既に修正済み。

### 3.1 `control-panel-smoke.spec.ts` — **完了 (ν-2.1)**

- 目的: 起動が通り、認証が疎通し、最小 UI が描画されることを確認。
- 実装: `/gui/?token=...` ロード → `<nav aria-label="Main tabs">` の "Live"
  ボタン可視 → `GET /api/v1/control/ping` に Bearer token を載せて 200。
- 実装メモ: `TabNav.svelte` は `<button>` + `<nav aria-label>` 構造で、`role="tab"`
  は使われていない。`getByRole('navigation', { name: 'Main tabs' })` でスコープを
  切り、その中の `getByRole('button', { name: /live/i })` を見る形に確定。

---

## 4. Fixture 設計: `conf.fixture.e2e.toml`

Rust 側 (`src/conf.rs`) に新規フィールドは足さない。**既存の conf スキーマだけで外部依存ゼロに組める** ことを fixture の正当性とする。

| key | 値 | 理由 |
|---|---|---|
| `workers` | `2` | E2E 側で並列度を落として再現性を上げる |
| `log_level` | `"Debug"` | 失敗時に `test-results/` に残ったログから回帰を追える |
| `web_ui_address` | `"127.0.0.1:57098"` | 開発の 57000 / dev server 5173 と衝突しない |
| `state_data_path` | 未設定（RON 永続化なし） | 各テストは空状態から始まる |
| `runtime_dir` | `"target/e2e-runtime"` | 一時ディレクトリを OS 既定から分離 |
| `flowgraph_dir` | `"tests/e2e/fixtures/flowgraph"` | 最小の sample flowgraph 同梱 |
| `gui_dist_path` | `"gui/dist"` | Playwright は build 済み dist を配信する VAC 経路で叩く |
| `[control_api]` | `token = "e2e-fixture-token"`、`require_token_for_loopback = true` | 認証経路も疎通確認の対象にする |
| 外部連携（Twitch / OpenAI / TTS / OCR / Voice / Translate） | **全セクション未設定** | 外部 IO ゼロ。API key 不要 |

補足:

- fixture の flowgraph は `tests/e2e/fixtures/flowgraph/sample.flowgraph.toml` として commit する。ノードは `v1.channel` + `flowgraph.util.debug_log` 程度の極小構成で、§3.2 の canvas 操作の足場にする。
- Dictionary の初期データは `flowgraph.example/` や既存 fixture から最小セットをコピー（§3.3 の 3-way merge を成立させる 1 エントリだけあれば良い）。
- fixture は **gui / からも Rust テストからも読めない位置**（`tests/e2e/fixtures/`）に置き、ユーザ配布物に混入しないようにする。

---

## 5. Architecture 判断

### 5.1 Playwright は gui/ に閉じる

`gui/package.json` の devDependency として `@playwright/test` を足し、`gui/playwright.config.ts` と `gui/tests/e2e/` を置く。Rust 側 `Cargo.toml` には一切触れない。

- 利点: Rust の CI / crate publish に影響しない。Playwright の browser binary は `gui/node_modules/` 配下に閉じ込められる。
- トレードオフ: `tests/e2e/fixtures/` を workspace root に置くか `gui/tests/e2e/fixtures/` に置くかは ν-1 で決定。**Rust 側の `cargo test` が間違えて拾わない** 場所が最優先なので、暫定は `gui/tests/e2e/fixtures/` を推す。

### 5.2 `webServer` 戦略

Playwright の `webServer` で VAC 本体を直接起動する:

```ts
webServer: [
  {
    command: 'cargo run --quiet --release --bin virtual-avatar-connect -- ../conf.fixture.e2e.toml',
    cwd: '..',
    url: 'http://127.0.0.1:57098/gui/',
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
],
```

- GUI 側の dev server (`vite`) は使わず、**本番と同じ `gui/dist` 経路** で叩く。これで「開発 proxy のせいで通るだけ」の偽陽性を避ける。
- ν-1 で Windows / Linux 両方で `cargo run` の `cwd` を通す。Windows の Defender / SmartScreen 初回 prompt を避けるため、ローカルの `./target/release/virtual-avatar-connect.exe` が既にビルドされていればそれを直接指す fallback も検討する（`webServer.command` の分岐）。

### 5.3 セレクタ戦略

`data-testid` を新設する前に、まず **accessible name / role** で書けるかを検討する:

- `getByRole('tab', { name: 'Flowgraph' })` のようにアクセシビリティ経路で探す。
- 無理な部分（Flowgraph Canvas の内部ノード / svelte-flow の生成 DOM）だけ `data-testid` を追加する。**network-wide `data-testid` の強制導入はしない**（ν の scope 膨張を避ける）。
- 追加する場合は `data-testid="vac-<area>-<role>"` 命名（例: `vac-flowgraph-node-<id>`）。

### 5.4 時刻・並列・スナップショット

- `workers: 1` から始める（race を避け、失敗時の bisect を簡単にする）。ν-2 の 5 ケースが全部通った後で並列化検討。
- `toHaveScreenshot` は ν-2 までは使わない。OS 固有のフォント差異で flake る。
- `test.setTimeout(60_000)` を default に置く（cold `cargo run --release` の起動込みで余裕を持たせる）。

---

## 6. Sub-phase Breakdown（確定）

| sub | 内容 | 触るもの |
|---|---|---|
| ν-0 | docs: 本書本文 + fixture 設計 / 5 ケース仕様 / 命名規約の確定 | `docs/roadmap/phase-nu-gui-e2e-playwright.md` + `docs/roadmap.md` ✅ |
| ν-1 | chore(gui): Playwright 導入 + `playwright.config.ts` + `webServer` + `conf.fixture.e2e.toml` + fixture flowgraph | `gui/package.json` / `gui/playwright.config.ts` / `gui/tests/e2e/fixtures/` / `conf.fixture.e2e.toml` ✅ |
| ν-2 | test(gui): 初期 5 ケースのうち **3 ケース** (§3.1 / §3.4 / §3.5) を実装 | `gui/tests/e2e/*.spec.ts` ✅ |
| ν-3 | docs: CHANGELOG / `docs/manual/` に run 手順 + CI optional 方針 + roadmap tick | `CHANGELOG.md` / `docs/manual/*` / `docs/roadmap.md` |
| ν-β | 残り 2 ケース (§3.2 canvas / §3.3 editor 409) を後続フェーズで着地 | `gui/tests/e2e/flowgraph-canvas-basic.spec.ts` / `gui/tests/e2e/dictionary-editor-409-merge.spec.ts` (未着手) |

各サブは 1 commit 1 トピックで分ける（Commit Granularity Rule）。

### 6.1 ν-1 チェックリスト

- [ ] `gui/package.json` に `@playwright/test` を devDependency 追加（`npm install -D @playwright/test`）
- [ ] `npx playwright install chromium` を README の手順に明記
- [ ] `gui/playwright.config.ts` 作成（`webServer` / `baseURL` / `testDir: 'tests/e2e'` / `reporter: [['list'], ['html', { open: 'never' }]]`）
- [ ] `gui/.gitignore` に `test-results/` / `playwright-report/` / `playwright/.cache/` を追加
- [ ] `conf.fixture.e2e.toml` を workspace root に配置し §4 のキー設定
- [ ] `gui/tests/e2e/fixtures/flowgraph/sample.flowgraph.toml` を配置
- [ ] `npm run test:e2e` / `npm run test:e2e:ui`（`--ui`）を `package.json` scripts に追加

### 6.2 ν-2 チェックリスト

- [x] §3.1 `control-panel-smoke.spec.ts`（accessible name のみ）
- [x] §3.4 `live-quick-add-learn-undo.spec.ts`（placeholder + role button + `waitForRequest`）
- [x] §3.5 `channels-ws-live-update.spec.ts`（`page.waitForEvent('websocket')` で frame 検証）
- [x] §3.2 `flowgraph-canvas-basic.spec.ts` **→ ν-β-2 で着地**
- [x] §3.3 `dictionary-editor-409-merge.spec.ts` **→ ν-β-1 で着地**
- [x] 全 spec が `npx playwright test` でローカル緑（5 specs / 1 worker で 9.3s）
- [ ] 5 spec の Linux / WSL 相互確認（ν+ で取り組む、Windows 側は本フェーズで確認済み）

### 6.3 ν-3 チェックリスト

- [x] CHANGELOG: `### ν: GUI E2E testing with Playwright` 節に ν-2 (3 specs) + ν-β 延期を追記（commit `4f8b15d`）
- [x] `docs/manual/` に `tutorials/gui-e2e.md` 相当（run 手順 + 失敗時の切り分け、commit `4f8b15d`）
- [x] `docs/roadmap.md` の Phase ν を "Active" → "Completed" に移し、ν-β を separate phase として起こす（commit `4f8b15d`、ν-β 側は本フェーズで Completed に再移動）

### 6.4 ν-β チェックリスト（後続フェーズ） — **完了**

- [x] §3.3 Dictionary Editor 409 merge: 別クライアントで hash を進めた上で merge dialog、force mine で解決（ν-β-1、commit `05b6a46`）
- [x] engine の `Table::empty()` default coerce 失敗を `Table::from_json_array(&[], None) → Table::empty()` で修正 + `PortSpec::with_default` round-trip test + fixture flowgraph から `dict_src` (`flowgraph.table.from_json`) を除去（ν-β-3、commit `e25b34d`）
- [x] §3.2 Flowgraph Canvas editor: Palette click-add + Save-dirty + Ctrl+S + PUT 往復（ν-β-2、commit `f31af7e`）。edge drag / beforeunload は ν+ へ送る

---

## 7. 非スコープ / 明示的にやらないこと

- 既存 GUI を大規模に書き換えること。`data-testid` は **必要最小限だけ** 追加する。
- 外部 IO を mock する仕組みの新設（Twitch / OpenAI）。fixture conf で全 disabled にすることで回避する。
- GitHub Actions matrix（chromium/firefox/webkit × Windows/Linux/macOS）。ν+。
- `toHaveScreenshot` による visual regression。ν+。
- Playwright Component Testing（Svelte コンポーネント単位のテスト）。ν+。

---

## 8. Risks

| risk | 対策 |
|---|---|
| `cargo run --release` の cold start が遅く、Playwright のタイムアウトで flake | `webServer.timeout = 180_000`、`reuseExistingServer: !CI` で local は再利用 |
| Windows の file lock で `target/release/*.exe` が掴まれて fixture 側再起動が失敗 | fixture の `runtime_dir` を `target/e2e-runtime` に分離し、PID も session_id で隔離 |
| svelte-flow の DOM 構造変更で selector が脆くなる | `data-testid` を最小限刺し、role 経路に寄せる（§5.3）|
| OS 固有フォント差分 | ν-2 時点で visual regression を使わない決定（§5.4） |
| VAC 起動ポート衝突（既に別プロセスが 57098 を使っている） | fixture を 57098 固定とし、conflict 時は開発者に kill を促すエラーメッセージ |
| Table 型 `PortSpec::with_default(SocketValue::Table(Table::empty()))` が engine で coerce されず `MissingRequiredInput` を吐く（§3.4 実装中に遭遇） | fixture 側に `flowgraph.table.from_json` (Pure) を挟んで空 Table を供給する回避策を採用。ν-β で engine 側を修正し fixture を簡素化する |

---

## 9. References

- [`../roadmap.md`](../roadmap.md) の "Phase ν" セクション
- [`phase-psi-alpha-encrypted-reasoning.md`](phase-psi-alpha-encrypted-reasoning.md)（直前フェーズ）
- [Playwright 公式](https://playwright.dev/)
- [Playwright + Svelte サンプル](https://playwright.dev/docs/frameworks#svelte)
