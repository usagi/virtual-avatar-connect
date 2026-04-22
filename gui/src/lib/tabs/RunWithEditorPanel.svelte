<script lang="ts">
 /**
  * Phase VI-γ-5b: `run_with` 編集パネル（Managed App の追加・削除）。
  *
  * - `api.runWithList()` で現状を取得。
  * - 新規追加フォーム: command / if_not_running / id / label / minimized / run_as_admin / working_dir。
  * - 各行の「削除」で `api.runWithDelete(index)`。conf.toml の該当エントリが除去される。
  * - 書き込みはサーバ側で妥当性検証 + バックアップされる（安全）。
  * - 起動済みプロセスへの影響はなし（= 削除しても走っているプロセスは止まらない）。
  * - run_with 形式が `[[run_with]]` テーブル形式で書かれている場合は API が 501 を返すので、
  *   UI はそのメッセージをそのまま表示する。
  */
 import { api } from '../api';
 import { ControlApiError } from '../types';
 import type { RunWithDto, RunWithTableDto, RunWithView } from '../types';
 import { toastStore } from '../toasts.svelte';

 let loading = $state(true);
 let error = $state<string | null>(null);
 let entries = $state<RunWithView[]>([]);
 let sourcePath = $state<string | null>(null);
 let submitting = $state(false);
 let busyIndex = $state<number | null>(null);

 // 新規追加フォーム
 let formCommand = $state('');
 let formIfNotRunning = $state('');
 let formId = $state('');
 let formLabel = $state('');
 let formWorkingDir = $state('');
 let formMinimized = $state(false);
 let formRunAsAdmin = $state(false);

 async function load() {
  loading = true;
  error = null;
  try {
   const res = await api.runWithList();
   entries = res.entries;
   sourcePath = res.source_path;
  } catch (e) {
   error = extractMessage(e);
  } finally {
   loading = false;
  }
 }

 $effect(() => {
  void load();
 });

 function extractMessage(e: unknown): string {
  if (e instanceof ControlApiError) {
   const body = e.body as { detail?: string; error?: string } | null;
   return body?.detail ?? body?.error ?? `${e.status} ${e.statusText}`;
  }
  if (e instanceof Error) return e.message;
  return String(e);
 }

 function resetForm() {
  formCommand = '';
  formIfNotRunning = '';
  formId = '';
  formLabel = '';
  formWorkingDir = '';
  formMinimized = false;
  formRunAsAdmin = false;
 }

 function buildDto(): RunWithDto | null {
  const cmd = formCommand.trim();
  if (!cmd) return null;
  // 何も追加オプションがない場合は純粋な文字列形式にできるが、
  // Managed App として扱うためには `if_not_running` を推奨するため、常に table 形式で送る。
  const table: RunWithTableDto = { command: cmd };
  if (formIfNotRunning.trim()) table.if_not_running = formIfNotRunning.trim();
  if (formId.trim()) table.id = formId.trim();
  if (formLabel.trim()) table.label = formLabel.trim();
  if (formWorkingDir.trim()) table.working_dir = formWorkingDir.trim();
  if (formMinimized) table.minimized = true;
  if (formRunAsAdmin) table.run_as_admin = true;
  return table;
 }

 async function addEntry() {
  const dto = buildDto();
  if (!dto) {
   toastStore.warn('command を入力してください');
   return;
  }
  submitting = true;
  try {
   const res = await api.runWithAdd(dto);
   entries = res.entries;
   sourcePath = res.source_path;
   toastStore.success(
    '連携アプリを追加しました',
    res.backup ? `バックアップ: ${res.backup}` : undefined,
   );
   resetForm();
  } catch (e) {
   toastStore.error('追加に失敗しました', extractMessage(e));
  } finally {
   submitting = false;
  }
 }

 async function deleteEntry(entry: RunWithView) {
  const ok = window.confirm(
   `${entry.display_label} (index=${entry.index}) を conf.toml から削除しますか？\n` +
    `既に起動済みのプロセスは影響を受けません。`,
  );
  if (!ok) return;
  busyIndex = entry.index;
  try {
   const res = await api.runWithDelete(entry.index);
   entries = res.entries;
   sourcePath = res.source_path;
   toastStore.success('連携アプリを削除しました', res.backup ? `バックアップ: ${res.backup}` : undefined);
  } catch (e) {
   toastStore.error('削除に失敗しました', extractMessage(e));
  } finally {
   busyIndex = null;
  }
 }

 function isTableEntry(dto: RunWithDto): dto is RunWithTableDto {
  return typeof dto !== 'string';
 }

 function entrySummary(view: RunWithView): string {
  if (isTableEntry(view.entry)) {
   return view.entry.command;
  }
  return view.entry;
 }
</script>

<section class="rounded border border-surface-200-800 bg-surface-50-950">
 <header class="flex items-center justify-between border-b border-surface-200-800 px-4 py-2">
  <div>
   <h3 class="text-sm font-semibold">連携アプリ設定（run_with）</h3>
   <p class="text-xs opacity-60">Managed Apps の登録・編集。追加後は「連携アプリ」ドロワーから起動・最小化・停止できます。</p>
  </div>
  <button
   type="button"
   class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-100-900"
   onclick={() => void load()}
   disabled={loading}
  >
   {loading ? '読込中…' : '更新'}
  </button>
 </header>

 {#if error}
  <p class="border-b border-error-500/40 bg-error-500/10 px-4 py-2 text-xs text-error-900-100">{error}</p>
 {/if}

 <div class="divide-y divide-surface-200-800">
  {#if entries.length === 0 && !loading && !error}
   <p class="px-4 py-3 text-xs opacity-60">登録済みエントリがありません。下のフォームから追加してください。</p>
  {:else}
   {#each entries as entry (entry.index)}
    {@const isBusy = busyIndex === entry.index}
    <div class="px-4 py-3 text-sm">
     <div class="flex flex-wrap items-start gap-2">
      <div class="min-w-0 flex-1">
       <div class="flex items-center gap-2">
        <span class="truncate font-medium">{entry.display_label}</span>
        <span class="rounded bg-surface-200-800 px-1.5 py-0 text-[10px] opacity-70">#{entry.index}</span>
        {#if entry.supports_status}
         <span class="rounded bg-primary-500/20 px-1.5 py-0.5 text-xs text-primary-500">monitorable</span>
        {:else}
         <span class="rounded bg-warning-500/20 px-1.5 py-0.5 text-xs text-warning-500" title="if_not_running 未指定のため状態監視不可">
          fire-and-forget
         </span>
        {/if}
       </div>
       <p class="mt-0.5 truncate font-mono text-[11px] opacity-60" title={entrySummary(entry)}>
        {entrySummary(entry)}
       </p>
       <div class="mt-1 flex flex-wrap gap-1 text-[10px] opacity-70">
        <code>id={entry.effective_id}</code>
        {#if isTableEntry(entry.entry)}
         {#if entry.entry.if_not_running}
          <code>if_not_running={entry.entry.if_not_running}</code>
         {/if}
         {#if entry.entry.minimized}
          <code>minimized</code>
         {/if}
         {#if entry.entry.run_as_admin}
          <code class="text-warning-500">admin</code>
         {/if}
         {#if entry.entry.working_dir}
          <code title={entry.entry.working_dir}>working_dir=…</code>
         {/if}
        {/if}
       </div>
      </div>
      <button
       type="button"
       class="rounded border border-error-500/40 px-2 py-0.5 text-xs text-error-500 hover:bg-error-500/10 disabled:opacity-50"
       onclick={() => void deleteEntry(entry)}
       disabled={isBusy}
      >
       削除
      </button>
     </div>
    </div>
   {/each}
  {/if}
 </div>

 <div class="border-t border-surface-200-800 bg-surface-100-900 p-4">
  <h4 class="mb-2 text-xs font-semibold opacity-80">新規追加</h4>
  <div class="grid grid-cols-1 gap-2 md:grid-cols-2">
   <label class="block text-xs">
    <span class="opacity-70">command（必須）</span>
    <input
     type="text"
     bind:value={formCommand}
     class="mt-0.5 w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs font-mono"
     placeholder="C:\path\to\app.exe --args"
     disabled={submitting}
    />
   </label>
   <label class="block text-xs">
    <span class="opacity-70">if_not_running（プロセス名, 任意）</span>
    <input
     type="text"
     bind:value={formIfNotRunning}
     class="mt-0.5 w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs font-mono"
     placeholder="app.exe"
     disabled={submitting}
    />
   </label>
   <label class="block text-xs">
    <span class="opacity-70">id（API 用安定 ID, 任意）</span>
    <input
     type="text"
     bind:value={formId}
     class="mt-0.5 w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
     placeholder="my-app"
     disabled={submitting}
    />
   </label>
   <label class="block text-xs">
    <span class="opacity-70">label（表示名, 任意）</span>
    <input
     type="text"
     bind:value={formLabel}
     class="mt-0.5 w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
     disabled={submitting}
    />
   </label>
   <label class="col-span-full block text-xs">
    <span class="opacity-70">working_dir（任意）</span>
    <input
     type="text"
     bind:value={formWorkingDir}
     class="mt-0.5 w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs font-mono"
     disabled={submitting}
    />
   </label>
   <label class="flex items-center gap-2 text-xs">
    <input type="checkbox" bind:checked={formMinimized} disabled={submitting} />
    <span>最小化で起動 (minimized)</span>
   </label>
   <label class="flex items-center gap-2 text-xs">
    <input type="checkbox" bind:checked={formRunAsAdmin} disabled={submitting} />
    <span>管理者権限で起動 (run_as_admin)</span>
   </label>
  </div>
  <div class="mt-3 flex items-center justify-end gap-2">
   <button
    type="button"
    class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-50-950"
    onclick={resetForm}
    disabled={submitting}
   >
    クリア
   </button>
   <button
    type="button"
    class="rounded bg-primary-500 px-3 py-1 text-xs font-semibold text-white hover:bg-primary-600 disabled:opacity-50"
    onclick={() => void addEntry()}
    disabled={submitting || !formCommand.trim()}
   >
    {submitting ? '追加中…' : '追加'}
   </button>
  </div>
 </div>

 {#if sourcePath}
  <footer class="border-t border-surface-200-800 px-4 py-2 text-xs opacity-60">
   書き込み先: {sourcePath}
  </footer>
 {/if}
</section>
