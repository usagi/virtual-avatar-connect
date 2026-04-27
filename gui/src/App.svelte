<script lang="ts">
 /**
  * Phase VI-γ-1: アプリケーションシェル。
  *
  * - 上部ヘッダ（ロゴ / 接続インジケータ / 再起動ボタン）
  * - 左ナビゲーション（Now / Modes / Flowgraph Studio / Resources / Observability / Settings）
  * - 下部ステータスバー（常駐）
  * - 右下トーストレイヤ
  * - 再起動・プロファイル切替モーダル
  *
  * 各タブの中身はコンテナコンポーネントに切り出し、本ファイルは薄い shell に徹する。
  *
  * δ-9 D.5: 旧 V1 `Pipeline` タブを廃止。Flowgraph タブに統合済み。
  */
 import ConnectionBadge from './lib/ConnectionBadge.svelte';
 import TabNav from './lib/TabNav.svelte';
 import StatusBar from './lib/StatusBar.svelte';
 import ToastLayer from './lib/ToastLayer.svelte';
 import ToastBridge from './lib/ToastBridge.svelte';
 import RestartDialog from './lib/RestartDialog.svelte';
 import ManagedAppDrawer from './lib/ManagedAppDrawer.svelte';

 import NowTab from './lib/tabs/NowTab.svelte';
 import ModesTab from './lib/tabs/ModesTab.svelte';
 import ResourcesTab from './lib/tabs/ResourcesTab.svelte';
 import SettingsTab from './lib/tabs/SettingsTab.svelte';
 import FlowgraphTab from './lib/tabs/FlowgraphTab.svelte';
 import LogsTab from './lib/tabs/LogsTab.svelte';

 import { tabNavStore } from './lib/tabs.svelte';
 import { api } from './lib/api';
 import { toastStore } from './lib/toasts.svelte';

 let restartDialogOpen = $state(false);
 let managedAppDrawerOpen = $state(false);
 let shutdownInFlight = $state(false);

 /**
  * Phase ε-1: VAC プロセスそのものを穏やかに終了させる。
  *
  * 停止は `POST /api/v1/control/shutdown`（= `ShutdownBroker` を trigger）に 1 本化している。
  * 成功レスポンス後はサーバが ManagedApp 停止 → actix graceful stop を順に実行するため、
  * WS は自然に切れる。GUI 側は停止画面を出さず、toast だけ出して静かに待機する。
  */
 async function handleShutdownClick() {
  if (shutdownInFlight) return;
  const ok = window.confirm(
   'VAC を終了します。連携アプリ（run_with で起動したもの）も停止を試み、\nブラウザの接続は切断されます。続行しますか？',
  );
  if (!ok) return;
  shutdownInFlight = true;
  try {
   const res = await api.shutdown();
   toastStore.info(
    'アプリ停止を要求しました',
    `pid=${res.current_pid} / status=${res.status}`,
   );
  } catch (e) {
   shutdownInFlight = false;
   toastStore.error('アプリ停止の要求に失敗しました', String(e));
  }
 }
</script>

<div class="vac-shell flex min-h-screen flex-col text-surface-950-50">
 <header class="vac-app-header sticky top-0 z-20 border-b backdrop-blur">
  <div class="flex items-center justify-between gap-3 px-6 py-2">
   <div class="flex items-baseline gap-3">
    <h1 class="text-lg font-bold">
     Virtual Avatar Connect
     <span class="text-primary-500">Runtime Cockpit</span>
    </h1>
    <span class="text-xs opacity-60">v2 GUI redesign</span>
   </div>
   <div class="flex items-center gap-2">
    <ConnectionBadge />
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
     title="連携アプリ（run_with）の起動/停止を管理"
     onclick={() => (managedAppDrawerOpen = true)}
    >
     連携アプリ…
    </button>
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
     title="VAC を再起動 / プロファイル切替"
     onclick={() => (restartDialogOpen = true)}
    >
     再起動…
    </button>
    <button
     type="button"
     class="rounded border border-error-500/60 px-3 py-1 text-xs text-error-500 hover:bg-error-500/10 disabled:opacity-50"
     title="VAC アプリを終了する（連携アプリの停止も試みる）"
     disabled={shutdownInFlight}
     onclick={handleShutdownClick}
    >
     {shutdownInFlight ? '停止中…' : '終了'}
    </button>
   </div>
  </div>
 </header>

 <div class="grid flex-1 min-h-0 lg:grid-cols-[240px_minmax(0,1fr)]">
  <aside class="vac-side-rail border-b lg:border-b-0 lg:border-r">
   <TabNav />
  </aside>

  <main class="vac-main-surface min-w-0 flex-1 px-4 py-4 lg:px-6">
   {#if tabNavStore.active === 'now'}
    <NowTab />
   {:else if tabNavStore.active === 'modes'}
    <ModesTab />
   {:else if tabNavStore.active === 'flowgraph'}
    <FlowgraphTab />
   {:else if tabNavStore.active === 'resources'}
    <ResourcesTab />
   {:else if tabNavStore.active === 'observability'}
    <LogsTab />
   {:else if tabNavStore.active === 'settings'}
    <SettingsTab />
   {/if}
  </main>
 </div>

 <StatusBar />
</div>

<ToastBridge />
<ToastLayer />
<RestartDialog bind:open={restartDialogOpen} />
<ManagedAppDrawer bind:open={managedAppDrawerOpen} />
