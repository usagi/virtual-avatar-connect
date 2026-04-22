<script lang="ts">
 // WebSocket 接続状態の小さなバッジ。
 // eventsStore は runes ベースなのでここで import するだけで自動追従する。
 import { eventsStore } from './events.svelte';

 const classByState: Record<string, string> = {
  idle: 'preset-tonal',
  connecting: 'preset-tonal-warning',
  open: 'preset-filled-success-500',
  closed: 'preset-filled-error-500',
  error: 'preset-filled-error-500',
 };

 const labelByState: Record<string, string> = {
  idle: '未接続',
  connecting: '接続中…',
  open: '接続',
  closed: '切断',
  error: 'エラー',
 };
</script>

<div class="flex items-center gap-2 text-xs">
 <span class="badge {classByState[eventsStore.connection]}">
  WS {labelByState[eventsStore.connection]}
 </span>
 <span class="opacity-60 mono">
  recv {eventsStore.received_count}
  {#if eventsStore.dropped_count > 0}
   / dropped {eventsStore.dropped_count}
  {/if}
 </span>
</div>
