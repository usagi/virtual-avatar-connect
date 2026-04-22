<script lang="ts">
 // 全体 / カテゴリ単位の Pause / Resume 操作パネル。
 //
 // 設計:
 //   - "全体" / "processors のみ" / "ais のみ" の 3 スコープを横並びボタンで提示
 //   - 各スコープは Pause / Resume の 2 ボタン持ち
 //   - 操作中は disabled + spinner、結果の PauseOutcome を toast 的に下に短く表示
 //   - 真の状態は `pause_state` イベント経由で SnapshotView が自動更新する
 //     （このパネルは "アクション発火装置" で、状態表示は SnapshotView に寄せる）
 //   - エラーは直近 1 件だけ保持し、次操作で消える

 import { api } from './api';
 import { ControlApiError, type PauseOutcome, type PauseTarget } from './types';

 type Scope = 'all' | 'processors' | 'ais';

 let busy: Scope | null = $state(null);
 let lastResult: { scope: Scope; verb: 'paused' | 'resumed'; outcome: PauseOutcome } | null = $state(null);
 let lastError: string | null = $state(null);

 async function act(scope: Scope, verb: 'pause' | 'resume'): Promise<void> {
  if (busy !== null) return;
  busy = scope;
  lastError = null;
  const target: PauseTarget = { target: scope };
  try {
   const outcome = verb === 'pause' ? await api.pause(target) : await api.resume(target);
   lastResult = { scope, verb: verb === 'pause' ? 'paused' : 'resumed', outcome };
  } catch (e) {
   lastError = e instanceof ControlApiError
    ? `${e.status} ${e.statusText}`
    : e instanceof Error
     ? e.message
     : String(e);
   lastResult = null;
  } finally {
   busy = null;
  }
 }

 const scopeLabel: Record<Scope, string> = {
  all: '全体',
  processors: 'Processors',
  ais: 'AI Personas',
 };

 const SCOPES: readonly Scope[] = ['all', 'processors', 'ais'];
</script>

<section class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
 <header class="mb-3 flex items-center justify-between">
  <h3 class="text-sm font-semibold">Pause / Resume</h3>
  <span class="text-[10px] opacity-60">サーバが soft suspend → 復帰可能</span>
 </header>

 <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
  {#each SCOPES as scope (scope)}
   <div class="rounded border border-surface-200-800 bg-surface-50-950 p-2">
    <div class="mb-1 text-xs font-semibold opacity-80">{scopeLabel[scope]}</div>
    <div class="flex gap-1">
     <button
      type="button"
      class="flex-1 rounded bg-warning-500/90 px-2 py-1 text-xs font-semibold text-warning-950 hover:bg-warning-500 disabled:opacity-50"
      onclick={() => act(scope, 'pause')}
      disabled={busy !== null}
     >
      {busy === scope ? '…' : '⏸ Pause'}
     </button>
     <button
      type="button"
      class="flex-1 rounded bg-success-500/90 px-2 py-1 text-xs font-semibold text-success-950 hover:bg-success-500 disabled:opacity-50"
      onclick={() => act(scope, 'resume')}
      disabled={busy !== null}
     >
      {busy === scope ? '…' : '▶ Resume'}
     </button>
    </div>
   </div>
  {/each}
 </div>

 {#if lastError}
  <p class="mt-2 rounded bg-error-200-800 px-2 py-1 text-xs text-error-900-100">
   <span class="font-semibold">ERROR:</span> {lastError}
  </p>
 {:else if lastResult}
  {@const r = lastResult}
  <p class="mt-2 rounded bg-surface-200-800 px-2 py-1 text-xs opacity-90">
   {scopeLabel[r.scope]} → <span class="font-semibold">{r.verb}</span>
   (procs: {r.outcome.processors_affected.length}, ais: {r.outcome.ais_affected.length})
  </p>
 {/if}
</section>
