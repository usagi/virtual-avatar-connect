<script lang="ts">
 // /api/v1/control/snapshot を取得し、processors / ai_personas / twitch を整形表示する。
 //
 // 更新戦略:
 //   - マウント時に 1 回取得
 //   - 10 秒ごとに自動リフレッシュ（polling）
 //   - `pause_state` / `reloaded` / `oauth_status` イベントを受けたら即時リフレッシュ
 //     （サーバ側の真実の状態と UI の即時同期を優先する）
 //
 // 設計メモ:
 //   - ここでは **表示のみ**。pause/resume 操作や reload は β-4/β-5 の別コンポーネントから叩く。
 //   - エラー時は赤い帯で表示し、古い snapshot は保持（flicker 防止）。

 import { onMount, onDestroy } from 'svelte';
 import { api } from './api';
 import { ControlApiError, type PauseTarget, type StateSnapshot } from './types';
 import { eventsStore } from './events.svelte';
 import PauseBadge from './PauseBadge.svelte';

 const AUTO_REFRESH_MS = 10_000;

 let snapshot: StateSnapshot | null = $state(null);
 let fetching: boolean = $state(false);
 let lastError: string | null = $state(null);
 let lastFetchedAt: number | null = $state(null);

 /** 個別トグル中の行キー ("proc:<idx>" / "ai:<idx>")。他の行のボタンは disable しない。 */
 let togglingKey: string | null = $state(null);
 let toggleError: string | null = $state(null);

 let timer: ReturnType<typeof setInterval> | null = null;
 let unsubscribe: (() => void) | null = null;

 async function refresh(): Promise<void> {
  if (fetching) return;
  fetching = true;
  try {
   snapshot = await api.snapshot();
   lastError = null;
   lastFetchedAt = Date.now();
  } catch (e) {
   lastError = e instanceof ControlApiError
    ? `${e.status} ${e.statusText}`
    : e instanceof Error
     ? e.message
     : String(e);
  } finally {
   fetching = false;
  }
 }

 onMount(() => {
  void refresh();
  timer = setInterval(() => void refresh(), AUTO_REFRESH_MS);

  // 状態変化イベントを受けたら snapshot を即時再取得
  unsubscribe = eventsStore.subscribe((ev) => {
   const k = ev.event.kind;
   if (k === 'pause_state' || k === 'reloaded' || k === 'oauth_status') {
    void refresh();
   }
  });
 });

 onDestroy(() => {
  if (timer !== null) clearInterval(timer);
  if (unsubscribe) unsubscribe();
 });

 const lastFetchedLabel = $derived(
  lastFetchedAt === null
   ? '-'
   : new Date(lastFetchedAt).toLocaleTimeString('ja-JP', { hour12: false })
 );

 /**
  * 1 行 pause/resume トグル。
  *
  * - 対象はインデックスで指定する（id は null 許容だが重複の可能性がある）
  * - 応答後に即 refresh を呼ぶ（pause_state イベント経由でも refresh されるが、UI 反応速度のため二重で叩く）
  */
 async function toggleOne(kind: 'processor' | 'ai', index: number, currentlyPaused: boolean): Promise<void> {
  const key = `${kind}:${index}`;
  if (togglingKey !== null) return;
  togglingKey = key;
  toggleError = null;
  const target: PauseTarget = { target: kind, index };
  try {
   if (currentlyPaused) {
    await api.resume(target);
   } else {
    await api.pause(target);
   }
   void refresh();
  } catch (e) {
   toggleError = e instanceof ControlApiError
    ? `${key}: ${e.status} ${e.statusText}`
    : e instanceof Error
     ? `${key}: ${e.message}`
     : `${key}: ${String(e)}`;
  } finally {
   togglingKey = null;
  }
 }
</script>

<section class="space-y-4">
 <header class="flex items-center justify-between">
  <h2 class="text-lg font-semibold">ランタイム状態</h2>
  <div class="flex items-center gap-2 text-xs opacity-70">
   <span>最終取得: {lastFetchedLabel}</span>
   <button
    type="button"
    class="rounded border border-surface-300-700 bg-surface-100-900 px-2 py-0.5 hover:bg-surface-200-800 disabled:opacity-50"
    onclick={() => void refresh()}
    disabled={fetching}
   >
    {fetching ? '…' : '更新'}
   </button>
  </div>
 </header>

 {#if lastError}
  <div class="rounded border border-error-500 bg-error-100-900 px-3 py-2 text-sm text-error-900-100">
   snapshot 取得に失敗: {lastError}
  </div>
 {/if}
 {#if toggleError}
  <div class="rounded bg-error-200-800 px-3 py-1 text-xs text-error-900-100">
   pause 操作失敗: {toggleError}
  </div>
 {/if}

 {#if snapshot}
  {@const s = snapshot}
  <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
   <!-- Runtime -->
   <div class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
    <h3 class="mb-2 text-sm font-semibold opacity-80">Runtime</h3>
    <dl class="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1 text-xs">
     <dt class="opacity-60">app_version</dt>
     <dd class="font-mono">{s.app_version}</dd>
     <dt class="opacity-60">schema</dt>
     <dd class="font-mono">{s.schema}</dd>
     <dt class="opacity-60">now</dt>
     <dd class="font-mono">{s.now}</dd>
     <dt class="opacity-60">session_id</dt>
     <dd class="truncate font-mono" title={s.runtime.session_id}>{s.runtime.session_id}</dd>
     <dt class="opacity-60">session_dir</dt>
     <dd class="truncate font-mono" title={s.runtime.session_dir}>{s.runtime.session_dir}</dd>
     <dt class="opacity-60">root</dt>
     <dd class="truncate font-mono" title={s.runtime.root}>{s.runtime.root}</dd>
     <dt class="opacity-60">inline_max_bytes</dt>
     <dd class="font-mono">{s.runtime.inline_max_bytes.toLocaleString()}</dd>
    </dl>
   </div>

   <!-- Twitch -->
   <div class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
    <h3 class="mb-2 text-sm font-semibold opacity-80">Twitch</h3>
    {#if s.twitch}
     {@const t = s.twitch}
     <dl class="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1 text-xs">
      <dt class="opacity-60">username</dt>
      <dd class="font-mono">{t.username}</dd>
      <dt class="opacity-60">channel_to</dt>
      <dd class="font-mono">{t.channel_to}</dd>
      <dt class="opacity-60">reads</dt>
      <dd class="font-mono">
       {t.reads && t.reads.length > 0 ? t.reads.join(', ') : '—'}
      </dd>
      <dt class="opacity-60">eventsub</dt>
      <dd>
       <PauseBadge paused={!t.eventsub_enabled} compact note={t.eventsub_enabled ? 'enabled' : 'disabled'} />
      </dd>
      <dt class="opacity-60">moderator</dt>
      <dd>
       {#if t.moderator_enabled}
        <span class="font-mono text-xs">{t.moderator_login ?? '?'} (enabled)</span>
       {:else}
        <span class="opacity-60 text-xs">disabled</span>
       {/if}
      </dd>
      <dt class="opacity-60">ignored</dt>
      <dd class="font-mono">
       {t.ignore_logins.length > 0 ? t.ignore_logins.join(', ') : '—'}
      </dd>
     </dl>
    {:else}
     <p class="text-xs opacity-60">Twitch 連携は無効</p>
    {/if}
   </div>
  </div>

  <!-- Processors -->
  <div class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
   <h3 class="mb-2 text-sm font-semibold opacity-80">
    Processors <span class="opacity-60">({s.processors.length})</span>
   </h3>
   {#if s.processors.length === 0}
    <p class="text-xs opacity-60">登録なし</p>
   {:else}
    <table class="w-full border-collapse text-xs">
     <thead>
      <tr class="border-b border-surface-300-700 text-left opacity-70">
       <th class="py-1 pr-2">#</th>
       <th class="py-1 pr-2">feature</th>
       <th class="py-1 pr-2">id</th>
       <th class="py-1 pr-2">state</th>
       <th class="py-1 pr-2">last invoked</th>
       <th class="py-1"></th>
      </tr>
     </thead>
     <tbody>
      {#each s.processors as p (p.index)}
       {@const key = `processor:${p.index}`}
       <tr class="border-b border-surface-200-800/40 last:border-b-0">
        <td class="py-1 pr-2 font-mono opacity-60">{p.index}</td>
        <td class="py-1 pr-2 font-mono">{p.feature}</td>
        <td class="py-1 pr-2 font-mono opacity-80">{p.id ?? '—'}</td>
        <td class="py-1 pr-2">
         <PauseBadge paused={p.paused} compact />
        </td>
        <td class="py-1 pr-2 font-mono opacity-70">
         {p.last_invoked_at ?? '—'}
        </td>
        <td class="py-1 text-right">
         <button
          type="button"
          class="rounded border border-surface-300-700 bg-surface-50-950 px-1.5 py-0 font-mono text-[10px] hover:bg-surface-200-800 disabled:opacity-50"
          title={p.paused ? 'この processor を再開' : 'この processor を一時停止'}
          onclick={() => void toggleOne('processor', p.index, p.paused)}
          disabled={togglingKey !== null}
         >
          {togglingKey === key ? '…' : p.paused ? '▶' : '⏸'}
         </button>
        </td>
       </tr>
      {/each}
     </tbody>
    </table>
   {/if}
  </div>

  <!-- AI Personas -->
  <div class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
   <h3 class="mb-2 text-sm font-semibold opacity-80">
    AI Personas <span class="opacity-60">({s.ai_personas.length})</span>
   </h3>
   {#if s.ai_personas.length === 0}
    <p class="text-xs opacity-60">登録なし</p>
   {:else}
    <ul class="space-y-1 text-xs">
     {#each s.ai_personas as a (a.index)}
      {@const key = `ai:${a.index}`}
      <li class="flex items-center gap-2">
       <span class="font-mono opacity-60">#{a.index}</span>
       <span class="font-mono">{a.id ?? '—'}</span>
       <PauseBadge paused={a.paused} compact />
       <button
        type="button"
        class="ml-auto rounded border border-surface-300-700 bg-surface-50-950 px-1.5 py-0 font-mono text-[10px] hover:bg-surface-200-800 disabled:opacity-50"
        title={a.paused ? 'この AI persona を再開' : 'この AI persona を一時停止'}
        onclick={() => void toggleOne('ai', a.index, a.paused)}
        disabled={togglingKey !== null}
       >
        {togglingKey === key ? '…' : a.paused ? '▶' : '⏸'}
       </button>
      </li>
     {/each}
    </ul>
   {/if}
  </div>
 {:else if !lastError}
  <p class="text-sm opacity-70">読み込み中…</p>
 {/if}
</section>
