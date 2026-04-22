<script lang="ts">
 /**
  * Phase VI-γ-2b: Managed App 持続ドロワー。
  *
  * - ヘッダ右肩の「連携アプリ」ボタンで開閉。
  * - 右側からスライドインし、アプリ一覧 + 各行の start/stop/minimize ボタンを置く。
  * - WebSocket `managed_app_state` を購読してリアルタイム反映。
  * - 操作ごとの軽量 toast を出す（api.ts 共通の error toast は request() 側で拾わないので、ここで投げる）。
  *
  * デザイン方針:
  *   - shell は普通のアプリ風（見慣れた drawer）を優先。アクション失敗は目立つ警告色で見せる。
  *   - 各行のボタンは「start = primary」「stop = error」「minimize = surface（Windows のみ）」の 3 つ。
  *   - `supports_status=false` の entry は「状態不明」として扱い、start のみ許可。
  */
 import { SvelteSet } from 'svelte/reactivity';
 import { api } from './api';
 import { eventsStore } from './events.svelte';
 import { toastStore } from './toasts.svelte';
 import {
  ControlApiError,
  type ControlEvent,
  type ManagedAppView,
  type ManagedAppsResponse,
 } from './types';

 interface Props {
  open: boolean;
 }
 let { open = $bindable() }: Props = $props();

 let loading = $state(true);
 let error = $state<string | null>(null);
 let resp = $state<ManagedAppsResponse | null>(null);
 /** 操作中の app id セット。ボタンの spinner 表示に使う。SvelteSet で細粒度リアクティブ。 */
 const busyIds = new SvelteSet<string>();

 async function load() {
  loading = true;
  error = null;
  try {
   resp = await api.managedApps();
  } catch (e) {
   error =
    e instanceof ControlApiError
     ? `${e.status} ${e.statusText}`
     : e instanceof Error
      ? e.message
      : String(e);
  } finally {
   loading = false;
  }
 }

 $effect(() => {
  // open になったときに取得。閉じている間は購読だけ効かせておき、開く都度リフレッシュ。
  if (open) {
   void load();
  }
 });

 // managed_app_state を受けたら対象 entry の status を上書き。全件再フェッチはしない（負荷軽減）。
 $effect(() => {
  const off = eventsStore.subscribe((ts) => {
   const ev: ControlEvent = ts.event;
   if (ev.kind !== 'managed_app_state') return;
   if (!resp) return;
   const idx = resp.entries.findIndex((e) => e.id === ev.id);
   if (idx < 0) return;
   // 新しい status で entry を差し替え（配列自体も新規化してリアクティブに拾わせる）
   const next = resp.entries.slice();
   next[idx] = {
    ...next[idx],
    status: {
     id: ev.id,
     running: ev.running,
     pids: ev.pids,
     checked_at: ev.checked_at,
    },
   };
   resp = { ...resp, entries: next };
  });
  return off;
 });

 function setBusy(id: string, busy: boolean) {
  if (busy) busyIds.add(id);
  else busyIds.delete(id);
 }

 async function actStart(entry: ManagedAppView) {
  setBusy(entry.id, true);
  try {
   const r = await api.managedAppStart(entry.id);
   if (r.was_running) {
    toastStore.warn(`${entry.label} は既に起動中です`);
   } else {
    toastStore.success(`${entry.label} を起動しました`);
   }
  } catch (e) {
   const msg = formatErr(e);
   if (e instanceof ControlApiError && e.status === 409) {
    toastStore.warn(`${entry.label} は既に起動中です`, msg);
   } else {
    toastStore.error(`${entry.label} の起動に失敗`, msg);
   }
  } finally {
   setBusy(entry.id, false);
  }
 }

 async function actStop(entry: ManagedAppView) {
  // 誤爆回避のため軽く確認
  if (!confirm(`${entry.label} を停止しますか？\nWM_CLOSE → 3 秒待機 → 強制終了 の順で試みます。`)) return;
  setBusy(entry.id, true);
  try {
   const r = await api.managedAppStop(entry.id, { grace_ms: 3000 });
   if (r.terminated_pids > 0) {
    toastStore.warn(
     `${entry.label} を強制終了しました`,
     `closed=${r.closed_windows}, terminated=${r.terminated_pids}`,
    );
   } else {
    toastStore.success(`${entry.label} にクローズ要求を送りました`, `closed_windows=${r.closed_windows}`);
   }
  } catch (e) {
   toastStore.error(`${entry.label} の停止に失敗`, formatErr(e));
  } finally {
   setBusy(entry.id, false);
  }
 }

 async function actRestart(entry: ManagedAppView) {
 if (!confirm(`${entry.label} を再起動しますか？\n（停止 → 起動 の順で実行します）`)) return;
 setBusy(entry.id, true);
 try {
  const r = await api.managedAppRestart(entry.id, { grace_ms: 3000 });
  const stoppedPart = r.was_running
   ? `stopped (closed=${r.closed_windows}, terminated=${r.terminated_pids})`
   : '起動していませんでした';
  toastStore.success(`${entry.label} を再起動しました`, stoppedPart);
 } catch (e) {
  toastStore.error(`${entry.label} の再起動に失敗`, formatErr(e));
 } finally {
  setBusy(entry.id, false);
 }
}

async function actMinimize(entry: ManagedAppView) {
  setBusy(entry.id, true);
  try {
   const r = await api.managedAppMinimize(entry.id);
   toastStore.info(`${entry.label} を最小化キューに入れました`, `pids=${r.scheduled_pids}`);
  } catch (e) {
   toastStore.error(`${entry.label} の最小化に失敗`, formatErr(e));
  } finally {
   setBusy(entry.id, false);
  }
 }

 function formatErr(e: unknown): string {
  if (e instanceof ControlApiError) return `${e.status} ${e.statusText}`;
  if (e instanceof Error) return e.message;
  return String(e);
 }

 function statusLabel(entry: ManagedAppView): string {
  if (!entry.supports_status) return '状態不明';
  return entry.status.running ? '起動中' : '停止';
 }
 function statusClass(entry: ManagedAppView): string {
  if (!entry.supports_status) return 'bg-surface-300-700 text-surface-900-100';
  return entry.status.running
   ? 'bg-success-300-700 text-success-900-100'
   : 'bg-error-300-700/60 text-error-900-100';
 }

 function close() {
  open = false;
 }

 function onBackdropKey(e: KeyboardEvent) {
  if (e.key === 'Escape') close();
 }
</script>

{#if open}
 <!-- backdrop: 半透明オーバーレイ。クリックで閉じる -->
 <div
  class="fixed inset-0 z-40 bg-surface-950/40 backdrop-blur-sm"
  role="button"
  tabindex="-1"
  aria-label="連携アプリドロワーを閉じる"
  onclick={close}
  onkeydown={onBackdropKey}
 ></div>
 <!-- drawer 本体 -->
 <aside
  class="fixed right-0 top-0 z-50 flex h-screen w-full max-w-md flex-col border-l border-surface-200-800 bg-surface-50-950 shadow-xl"
  aria-label="Managed app drawer"
 >
  <header class="flex items-center justify-between border-b border-surface-200-800 px-4 py-3">
   <div>
    <h2 class="text-sm font-semibold">連携アプリ</h2>
    <p class="text-xs opacity-60">run_with で管理するアプリの監視と操作</p>
   </div>
   <div class="flex items-center gap-2">
    <button
     type="button"
     class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900"
     onclick={() => void load()}
     disabled={loading}
    >
     再読込
    </button>
    <button
     type="button"
     class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900"
     aria-label="閉じる"
     onclick={close}
    >
     ×
    </button>
   </div>
  </header>

  <div class="flex-1 overflow-y-auto p-3">
   {#if loading}
    <p class="text-xs opacity-70">読み込み中…</p>
   {:else if error}
    <p class="rounded border border-error-500/40 bg-error-500/10 p-2 text-xs">{error}</p>
   {:else if !resp || resp.entries.length === 0}
    <p class="rounded border border-warning-500/40 bg-warning-500/10 p-2 text-xs">
     登録済みの Managed App がありません。<br />
     <code>conf.toml</code> の <code>[[run_with]]</code> エントリに
     <code>if_not_running = "プロセス名"</code> を書いてください。
    </p>
   {:else}
    <ul class="space-y-2">
     {#each resp.entries as entry (entry.id)}
      {@const busy = busyIds.has(entry.id)}
      {@const canStop = entry.supports_status && entry.status.running}
      {@const canMinimize = entry.supports_status && entry.status.running}
      <li class="rounded border border-surface-200-800 bg-surface-100-900 p-3">
       <div class="flex items-start justify-between gap-2">
        <div class="min-w-0 flex-1">
         <div class="flex items-center gap-2">
          <span class="truncate text-sm font-semibold">{entry.label}</span>
          <span class="rounded px-1.5 py-0 text-[10px] font-semibold uppercase {statusClass(entry)}">
           {statusLabel(entry)}
          </span>
         </div>
         <p class="mt-0.5 truncate font-mono text-[11px] opacity-60" title={entry.command}>
          {entry.command}
         </p>
         <div class="mt-1 flex flex-wrap gap-1 text-[10px] opacity-70">
          <code>id={entry.id}</code>
          {#if entry.process_marker}
           <code>marker={entry.process_marker}</code>
          {/if}
          {#if entry.minimized}
           <code>minimized</code>
          {/if}
          {#if entry.run_as_admin}
           <code class="text-warning-500">admin</code>
          {/if}
          {#if entry.supports_status && entry.status.running && entry.status.pids.length > 0}
           <code>pids=[{entry.status.pids.join(',')}]</code>
          {/if}
         </div>
        </div>
       </div>

       <div class="mt-2 flex flex-wrap gap-1.5">
        <button
         type="button"
         class="rounded bg-primary-500 px-2 py-1 text-xs font-semibold text-white hover:bg-primary-600 disabled:opacity-50"
         onclick={() => void actStart(entry)}
         disabled={busy || (entry.supports_status && entry.status.running)}
        >
         起動
        </button>
        <button
         type="button"
         class="rounded border border-error-500 px-2 py-1 text-xs font-semibold text-error-500 hover:bg-error-500 hover:text-white disabled:opacity-30"
         onclick={() => void actStop(entry)}
         disabled={busy || !canStop}
        >
         停止
        </button>
        <button
         type="button"
         class="rounded border border-warning-500 px-2 py-1 text-xs font-semibold text-warning-500 hover:bg-warning-500 hover:text-white disabled:opacity-30"
         onclick={() => void actRestart(entry)}
         disabled={busy || !entry.supports_status}
         title="停止 → 起動 の連続操作"
        >
         再起動
        </button>
        <button
         type="button"
         class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-200-800 disabled:opacity-30"
         onclick={() => void actMinimize(entry)}
         disabled={busy || !canMinimize}
         title="Windows 専用"
        >
         最小化
        </button>
       </div>
      </li>
     {/each}
    </ul>
   {/if}
  </div>
 </aside>
{/if}
