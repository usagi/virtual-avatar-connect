# VAC Control Panel (Phase VI-β)

[Virtual Avatar Connect](../README.md) の **Phase VI-α Control API** を使って、起動中の VAC を
ブラウザから監視・操作するための Web GUI。

## 技術スタック

| 層 | 選定 |
|----|------|
| Framework | Svelte 5 (runes) + Vite + TypeScript |
| Node editor | [Svelte Flow](https://svelteflow.dev/) (`@xyflow/svelte`) |
| UI kit | [Skeleton v4](https://skeleton.dev/) (内部で [melt-ui](https://melt-ui.com/) を使う headless 基盤) + Tailwind v4 |
| 将来 | そのまま Tauri v2 に持ち込める構成 |

TypeScript を採用しているのは、VAC 本体の `ControlEvent` / `ReloadRequest` などが `#[serde(tag = "...")]`
付きの Rust tagged enum で定義されており、**TS の discriminated union へ 1:1 に写して WebSocket ハンドラの
網羅性を compile-time にチェックしたい**のが決め手。I/O 境界の型擬態 (`fetch().json()` が `any` を返す等)
には valibot / zod で後追い runtime validation を噛ませる方針。

## 開発

```powershell
# 1) VAC 本体を別ターミナルで起動しておく（既定 127.0.0.1:57000）
cargo run

# 2) GUI の dev サーバ（localhost:5173、VAC に proxy する）
cd gui
npm install
npm run dev
```

ブラウザで <http://localhost:5173/gui/> を開くと Control Panel が立ち上がる。
`/api/v1/control/*` と `/websocket` は Vite が VAC backend に proxy する。

**LAN や別ホスト上の VAC に向けたい場合**:

```powershell
$env:VAC_BACKEND_URL = "http://192.168.0.10:57000"
npm run dev
```

## ビルドと配信

```powershell
npm run build       # gui/dist/ に静的アセットを生成
```

ビルド後、VAC 本体を起動するとそのまま <http://127.0.0.1:57000/gui/> で Control Panel が配信される
（Phase VI-β-9 で統合済み）。配信元ディレクトリは `conf.toml` の `gui_dist_path`（既定 `gui/dist`）で
変更できる。未ビルド時は `/gui/*` が「ビルド手順を案内する HTML」を返すので、GUI 未使用の運用でも
VAC プロセスは問題なく起動する。

dev ワークフロー（GUI にホットリロードを効かせつつ VAC の挙動を追いたい）では従来どおり
`npm run dev` と VAC を並走させる。

## 型チェック / Lint

```powershell
npm run check       # svelte-check + tsc
npm run lint        # ESLint (TypeScript + Svelte 5)
npm run lint:fix    # --fix 付き
```

ESLint には **`.ts` / `.js` ファイル内での Svelte 5 rune (`$state` / `$derived` / `$effect` など) 使用を
error 扱いにするカスタムルール**を仕込んでいる。runes は `vite-plugin-svelte` によって変換されるため、
`.svelte` / `.svelte.ts` / `.svelte.js` 以外のファイルで使うと **runtime で `rune_outside_svelte` エラー
になる**（TypeScript も svelte-check もビルドも通ってしまう罠）ので、lint で水際検出している。

## Phase VI-β 進捗

- [x] **β-1**: Scaffold + Tailwind v4 + Skeleton v4 + Svelte Flow + melt-ui 導入
- [x] **β-2**: Control API クライアント (`lib/api.ts`) と WebSocket 購読ストア (`lib/events.svelte.ts`)
- [x] **β-3**: Dashboard (snapshot 表示 + ライブイベントストリーム + pause バッジ)
- [x] **β-4**: Pause / Resume UI
- [x] **β-5**: Reload UI (AI instructions / heartbeat / decision threshold / modify files)
- [x] **β-6**: OAuth Twitch UI
  - 起動時点で保存トークンが有効なら authorized バッジを即時表示（DCF を開始する前から分かる）
  - これは `StateSnapshot.twitch.{broadcaster,moderator}_authorized` を参照することで実現
  - 認可済みカードに警告色の「⚠ トークンキャッシュを削除」ボタン (`DELETE /oauth/twitch/{account}/tokens`)
- [x] **β-7**: メイン GUI 入力 ingress (`POST /api/v1/control/ingress`)
  - channel / content / is_final / flags / source / meta を完全指定可能
  - WebInput processor の設定 (`[[processors]] feature="webinput"`) に依存しない恒常的な入力口
  - localStorage に直近 channel を記憶、送信履歴 10 件を保持
- [x] **β-8**: Node-based pipeline editor（Svelte Flow 初版）
  - `snapshot.processors` / `snapshot.ai_personas` を水平チェインで可視化
  - バックエンドに `ControlEvent::ProcessorInvoked { index, feature, id, channel_datum_id, trigger_channel, elapsed_ms, outcome }` を追加し、`dispatch_processors_for_incoming` で逐次発射
  - 到着した `processor_invoked` で該当ノードをパルス（緑=continued / 橙=break / 赤=error）。`channel_datum` は Ingress ノードを光らせる
  - 暫定エッジは宣言順のチェイン。将来 `is_channel_from` / 出力 channel をバックエンド DTO に公開し、実際の channel 依存グラフに置き換える
- [x] **β-9**: VAC 本体から `/gui/*` で静的配信
  - 配信元は `conf.toml` の `gui_dist_path`（既定 `gui/dist`）
  - 未ビルド時は `/gui/*` が親切なビルド手順案内 HTML を返す（VAC 本体は起動できる）

## Phase VI-γ 進捗

- [ ] **γ-0**: 情報アーキテクチャの合意（完了）
- [x] **γ-1**: ナビゲーション骨格 + 再起動 API + プロファイル切替 + ウィジェット下地
  - アプリシェル再構成: 上部ヘッダ（ロゴ / 接続バッジ / 再起動ボタン）+ 5 タブ（Live / Setup / Pipeline / Logs / Tools）+ 下部常駐ステータスバー + 右下トーストレイヤ
  - URL hash (`#live`, `#pipeline` …) でタブ deep-link、ブラウザ戻る/進むと同期
  - バックエンド: `POST /api/v1/control/restart`（current_exe を新 conf で spawn → graceful_ms 後 exit）、`GET /api/v1/control/profiles`（同ディレクトリの `*.toml` 列挙 + 現行マーク）
  - `ControlEvent::Restarting { new_conf_path, new_pid, graceful_ms, current_pid }` を WS に追加。GUI は受信でトーストを出し、`confSyncStore` を restart_in_flight に遷移 → 新プロセスの最初の `Heartbeat` で synced に戻す
  - 新プロファイルは現 conf と同ディレクトリ限定（`..` 脱出禁止）+ `Conf::new_noop_probe` で事前検証、deserialize に失敗する conf は 400 で弾く
  - `WidgetSlot.svelte`: 将来の OBS/Twitch 連携ウィジェット用の差し込み枠（γ-1 では placeholder のみ）
  - `confSyncStore.svelte.ts`: synced / dirty / saved_restart_needed / restart_in_flight の状態機械。γ-4a で本格稼働
  - 既存パネル (SnapshotView / PausePanel / ReloadPanel / OAuthPanel / IngressPanel / EventStream) は各タブに再配置（Live → Ingress、Setup → OAuth/Reload/Pause、Logs → Snapshot/EventStream、Tools → Ping）
- [ ] **γ-2**: Live タブ本体（BOS iframe + URL コピー + Managed App ドロワー + 大型 Pause）
- [ ] **γ-3**: Setup タブ整理（Voice / AI / Twitch / run_with）
- [ ] **γ-4a**: Pipeline ビジュアルエディタ基盤（ノード追加/削除/並び替え + ジェネリック TOML/JSON プロパティエディタ + `toml_edit` 書き戻し + `.layout.json` によるノード位置保存）
- [ ] **γ-4b**: 主要 feature のリッチ UI（modify / openai_chat / TTS / ai.personas）
- [ ] **γ-5**: Logs タブ（高度フィルタ / ダウンロード / 全文検索）
- [ ] **γ-6**: Tools タブ整理
- [ ] **γ-7**: UX 調整（文言 / ショートカット / 色 / アイコン）

## 認証まわり

Control API の認証ポリシー（loopback は無認証 / LAN は Bearer 必須が既定）はそのままこの GUI にも
適用される。dev サーバは loopback 経由で proxy しているので基本的にトークン不要。LAN からアクセスする
場合は、GUI 側で Bearer を組み立てる実装を β-2 で足す予定。
