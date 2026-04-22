<script lang="ts">
 // 疎通確認コンポーネント（ヘッダ用の compact 表示）。
 //
 // /ping と /whoami を並行取得し、1 行のバッジに:
 //   - OK: VAC <version> · <loopback|LAN> · <token required|no token>
 //   - ERROR: <message>
 // を表示する。詳細（WebSocket 状態 / processors 一覧）は ConnectionBadge と SnapshotView に任せる。

 import { api } from './api';
 import type { PingResponse, WhoAmIResponse } from './types';

 type FetchState = 'loading' | 'ok' | 'error';

 let status: FetchState = $state('loading');
 let ping: PingResponse | null = $state(null);
 let whoami: WhoAmIResponse | null = $state(null);
 let error: string | null = $state(null);

 async function refresh(): Promise<void> {
  status = 'loading';
  error = null;
  try {
   const [p, w] = await Promise.all([api.ping(), api.whoami()]);
   ping = p;
   whoami = w;
   status = 'ok';
  } catch (e) {
   error = e instanceof Error ? e.message : String(e);
   status = 'error';
  }
 }

 $effect(() => {
  void refresh();
 });

 const summary = $derived.by(() => {
  if (status !== 'ok' || !ping || !whoami) return '';
  const net = whoami.is_loopback ? 'loopback' : 'LAN';
  const tok = whoami.required_token ? 'token required' : 'no token';
  return `VAC ${ping.version} · ${net} · ${tok}`;
 });
</script>

<div class="flex items-center gap-2 text-xs">
 {#if status === 'loading'}
  <span class="rounded bg-surface-300-700 px-2 py-0.5 font-mono uppercase">…</span>
  <span class="opacity-70">疎通確認中</span>
 {:else if status === 'ok'}
  <span class="rounded bg-success-200-800 px-2 py-0.5 font-mono font-semibold uppercase text-success-900-100">
   OK
  </span>
  <span class="font-mono opacity-80" title={JSON.stringify({ ping, whoami }, null, 2)}>
   {summary}
  </span>
 {:else}
  <span class="rounded bg-error-200-800 px-2 py-0.5 font-mono font-semibold uppercase text-error-900-100">
   ERR
  </span>
  <span class="truncate font-mono opacity-80" title={error ?? ''} style="max-width: 24rem;">
   {error}
  </span>
 {/if}

 <button
  type="button"
  class="rounded border border-surface-300-700 bg-surface-100-900 px-2 py-0.5 hover:bg-surface-200-800"
  onclick={() => void refresh()}
  title="/ping と /whoami を再取得"
 >
  ↻
 </button>
</div>
