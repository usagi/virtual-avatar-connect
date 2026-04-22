<script lang="ts">
 /**
  * 常駐ステータスバー（画面下端）。
  *
  * 表示項目:
  *   - WS 接続状態
  *   - snapshot 由来: AI persona 数、pause 中数
  *   - 直近 heartbeat からの経過時間
  *   - conf sync phase（γ-4a で本格稼働）
  *
  * δ-9 D.5: V1 `PROC` / `FIRE` インジケータは廃止（processor dispatch ビューは Flowgraph タブ側に移行）。
  */
 import { eventsStore } from './events.svelte';
 import { api } from './api';
 import { confSyncStore } from './confSync.svelte';
 import type { StateSnapshot } from './types';

 let snapshot = $state<StateSnapshot | null>(null);
 let lastHeartbeatAt: number | null = $state(null);
 let now = $state(Date.now());

 // 起動時に snapshot を 1 度取り、WS から heartbeat を拾う。
 // γ-2 で pause_state/reloaded 等でも refresh する。
 async function refreshSnapshot() {
  try {
   snapshot = await api.snapshot();
  } catch {
   // ステータスバーは軽量であるべきなので、エラーは飲む（Logs タブで拾う）
  }
 }

 $effect(() => {
  void refreshSnapshot();
  const off = eventsStore.subscribe((ts) => {
   if (ts.event.kind === 'heartbeat') {
    lastHeartbeatAt = ts.received_at;
   } else if (ts.event.kind === 'pause_state' || ts.event.kind === 'reloaded') {
    void refreshSnapshot();
   }
  });
  const tick = setInterval(() => {
   now = Date.now();
  }, 1000);
  return () => {
   off();
   clearInterval(tick);
  };
 });

 const aiCount = $derived(snapshot?.ai_personas.length ?? 0);
 const aiPausedCount = $derived(snapshot?.ai_personas.filter((a) => a.paused).length ?? 0);
 const hbAgeSec = $derived(lastHeartbeatAt ? Math.max(0, Math.floor((now - lastHeartbeatAt) / 1000)) : null);
 const wsConn = $derived(eventsStore.connection);
 const wsColor = $derived(
  wsConn === 'open'
   ? 'bg-success-500'
   : wsConn === 'connecting'
    ? 'bg-warning-500'
    : 'bg-error-500',
 );

 const confPhaseLabel = $derived.by(() => {
  switch (confSyncStore.phase) {
   case 'synced':
    return { text: 'synced', tone: 'text-success-500' };
   case 'dirty':
    return { text: 'dirty', tone: 'text-warning-500' };
   case 'saved_restart_needed':
    return { text: 'restart needed', tone: 'text-warning-500' };
   case 'restart_in_flight':
    return { text: 'restarting…', tone: 'text-primary-500' };
  }
 });
</script>

<footer
 class="sticky bottom-0 z-30 flex flex-wrap items-center justify-between gap-x-4 gap-y-1 border-t border-surface-200-800 bg-surface-50-950/95 px-6 py-1.5 text-xs backdrop-blur"
>
 <div class="flex flex-wrap items-center gap-x-4 gap-y-1">
  <span class="flex items-center gap-1.5" title="WebSocket connection">
   <span class="inline-block h-2 w-2 rounded-full {wsColor}"></span>
   <span class="opacity-70">WS</span>
   <span>{wsConn}</span>
  </span>
  <span title="AI personas (active / total)">
   <span class="opacity-70">AI</span>
   <span class="ml-1 tabular-nums">{aiCount - aiPausedCount} / {aiCount}</span>
   {#if aiPausedCount > 0}
    <span class="ml-1 text-warning-500">({aiPausedCount} paused)</span>
   {/if}
  </span>
  <span title="heartbeat age">
   <span class="opacity-70">HB</span>
   {#if hbAgeSec === null}
    <span class="ml-1 opacity-60">—</span>
   {:else}
    <span class="ml-1 tabular-nums">{hbAgeSec}s</span>
   {/if}
  </span>
 </div>
 <div class="flex items-center gap-x-4">
  <span class={confPhaseLabel.tone} title="conf sync state">
   <span class="opacity-70">CONF</span>
   <span class="ml-1">{confPhaseLabel.text}</span>
  </span>
 </div>
</footer>
