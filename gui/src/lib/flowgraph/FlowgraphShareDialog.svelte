<script lang="ts">
 /**
  * Phase δ-7e: Fragment paste / ZIP import のための汎用ダイアログ。
  *
  * 4 モード:
  *   - `paste`      : fragment TOML を貼り付けて指定ファイル/フォルダに paste
  *   - `import_zip` : ZIP ファイルを選んで dry_run → 本番 import の 2 段階
  *
  * 親は `mode` と `open` を bind する。閉じた時点でフォームはリセットされる。
  */
 import { flowgraphStore } from '../flowgraphStore.svelte';
 import type {
  FragmentOnConflict,
  FragmentOnConflictFile,
  ZipImportPreview,
 } from '../types';

 type Props = {
  open: boolean;
  mode: 'paste' | 'import_zip';
 };

 let { open = $bindable(), mode = $bindable() }: Props = $props();

 // --- Paste state ---
 let pasteToml = $state('');
 let pasteTargetKind = $state<'file' | 'folder'>('file');
 let pasteTargetFq = $state('');
 let pasteTargetFolder = $state('');
 let onConflictNode = $state<FragmentOnConflict>('suffix');
 let onConflictFile = $state<FragmentOnConflictFile>('rename');
 let offsetX = $state(0);
 let offsetY = $state(0);

 // --- ZIP state ---
 let zipFile = $state<File | null>(null);
 let zipTargetPrefix = $state('');
 let zipOnConflictNode = $state<FragmentOnConflict>('suffix');
 let zipOnConflictFile = $state<FragmentOnConflictFile>('rename');
 let zipPreview = $state<ZipImportPreview | null>(null);
 let zipBusy = $state(false);

 function reset() {
  pasteToml = '';
  pasteTargetKind = 'file';
  pasteTargetFq = flowgraphStore.currentFq ?? '';
  pasteTargetFolder = '';
  onConflictNode = 'suffix';
  onConflictFile = 'rename';
  offsetX = 0;
  offsetY = 0;
  zipFile = null;
  zipTargetPrefix = '';
  zipOnConflictNode = 'suffix';
  zipOnConflictFile = 'rename';
  zipPreview = null;
  zipBusy = false;
 }

 function close() {
  open = false;
  reset();
 }

 async function onPaste() {
  if (!pasteToml.trim()) return;
  const target =
   pasteTargetKind === 'file'
    ? ({ kind: 'file', fq: pasteTargetFq } as const)
    : ({ kind: 'folder', path: pasteTargetFolder } as const);
  const offset: [number, number] | null =
   offsetX !== 0 || offsetY !== 0 ? [offsetX, offsetY] : null;
  const resp = await flowgraphStore.fragmentPaste({
   fragment_toml: pasteToml,
   target,
   options: {
    on_conflict_node: onConflictNode,
    on_conflict_file: onConflictFile,
    position_offset: offset,
   },
  });
  if (resp) close();
 }

 async function onFilePicked(ev: Event) {
  const input = ev.target as HTMLInputElement;
  zipFile = input.files?.[0] ?? null;
  zipPreview = null;
  if (!zipFile) return;
  zipBusy = true;
  const outcome = await flowgraphStore.importZip(zipFile, {
   dry_run: true,
   target_prefix: zipTargetPrefix || undefined,
   on_conflict_node: zipOnConflictNode,
   on_conflict_file: zipOnConflictFile,
  });
  zipBusy = false;
  if (outcome && outcome.kind === 'preview') zipPreview = outcome;
 }

 async function onImportApply() {
  if (!zipFile) return;
  zipBusy = true;
  const outcome = await flowgraphStore.importZip(zipFile, {
   dry_run: false,
   target_prefix: zipTargetPrefix || undefined,
   on_conflict_node: zipOnConflictNode,
   on_conflict_file: zipOnConflictFile,
  });
  zipBusy = false;
  if (outcome && outcome.kind === 'report') close();
 }

 $effect(() => {
  if (open && pasteTargetFq === '' && flowgraphStore.currentFq) {
   pasteTargetFq = flowgraphStore.currentFq;
  }
 });
</script>

{#if open}
 <div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
  role="dialog"
  aria-modal="true"
 >
  <div
   class="flex max-h-[90vh] w-[min(640px,95vw)] flex-col gap-3 overflow-y-auto rounded-lg bg-surface-50-950 p-4 shadow-xl"
  >
   <div class="flex items-center justify-between">
    <h2 class="text-lg font-semibold">
     {#if mode === 'paste'}Fragment Paste{:else}ZIP Import{/if}
    </h2>
    <button
     type="button"
     class="rounded px-2 py-1 text-sm hover:bg-surface-200-800"
     onclick={close}>✕</button
    >
   </div>

   {#if mode === 'paste'}
    <label class="flex flex-col gap-1 text-xs">
     <span>Fragment TOML</span>
     <textarea
      class="min-h-[180px] resize-y rounded border border-surface-300-700 bg-surface-100-900 p-2 font-mono text-xs"
      placeholder="クリップボードの fragment TOML を貼り付け…"
      bind:value={pasteToml}
     ></textarea>
    </label>

    <div class="flex items-center gap-2 text-xs">
     <label class="flex items-center gap-1">
      <input type="radio" bind:group={pasteTargetKind} value="file" />
      File
     </label>
     <label class="flex items-center gap-1">
      <input type="radio" bind:group={pasteTargetKind} value="folder" />
      Folder
     </label>
    </div>

    {#if pasteTargetKind === 'file'}
     <label class="flex flex-col gap-1 text-xs">
      <span>Target fq (e.g. <code>project/main.flowgraph</code>)</span>
      <input
       type="text"
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5 font-mono text-xs"
       bind:value={pasteTargetFq}
      />
     </label>
    {:else}
     <label class="flex flex-col gap-1 text-xs">
      <span>Target folder (root 相対パス、空文字で root)</span>
      <input
       type="text"
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5 font-mono text-xs"
       bind:value={pasteTargetFolder}
      />
     </label>
    {/if}

    <div class="grid grid-cols-2 gap-2 text-xs">
     <label class="flex flex-col gap-1">
      <span>on_conflict_node</span>
      <select
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5"
       bind:value={onConflictNode}
      >
       <option value="suffix">suffix</option>
       <option value="skip">skip</option>
       <option value="overwrite">overwrite</option>
      </select>
     </label>
     <label class="flex flex-col gap-1">
      <span>on_conflict_file</span>
      <select
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5"
       bind:value={onConflictFile}
      >
       <option value="rename">rename</option>
       <option value="skip">skip</option>
       <option value="overwrite">overwrite</option>
       <option value="merge">merge</option>
      </select>
     </label>
     <label class="flex flex-col gap-1">
      <span>offset x</span>
      <input
       type="number"
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5"
       bind:value={offsetX}
      />
     </label>
     <label class="flex flex-col gap-1">
      <span>offset y</span>
      <input
       type="number"
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5"
       bind:value={offsetY}
      />
     </label>
    </div>

    <div class="flex justify-end gap-2 pt-2">
     <button
      type="button"
      class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800"
      onclick={close}>Cancel</button
     >
     <button
      type="button"
      class="rounded bg-primary-500 px-3 py-1 text-xs font-semibold text-white hover:bg-primary-600 disabled:opacity-40"
      disabled={!pasteToml.trim() ||
       flowgraphStore.mutating ||
       (pasteTargetKind === 'file' && !pasteTargetFq.trim())}
      onclick={onPaste}
     >
      Paste
     </button>
    </div>
   {:else}
    <label class="flex flex-col gap-1 text-xs">
     <span>ZIP file</span>
     <input
      type="file"
      accept=".zip,application/zip"
      class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5 text-xs"
      onchange={onFilePicked}
     />
    </label>

    <label class="flex flex-col gap-1 text-xs">
     <span>Target prefix (root 相対パス、空文字で root)</span>
     <input
      type="text"
      class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5 font-mono text-xs"
      bind:value={zipTargetPrefix}
     />
    </label>

    <div class="grid grid-cols-2 gap-2 text-xs">
     <label class="flex flex-col gap-1">
      <span>on_conflict_node</span>
      <select
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5"
       bind:value={zipOnConflictNode}
      >
       <option value="suffix">suffix</option>
       <option value="skip">skip</option>
       <option value="overwrite">overwrite</option>
      </select>
     </label>
     <label class="flex flex-col gap-1">
      <span>on_conflict_file</span>
      <select
       class="rounded border border-surface-300-700 bg-surface-100-900 p-1.5"
       bind:value={zipOnConflictFile}
      >
       <option value="rename">rename</option>
       <option value="skip">skip</option>
       <option value="overwrite">overwrite</option>
       <option value="merge">merge</option>
      </select>
     </label>
    </div>

    {#if zipPreview}
     <div class="rounded border border-surface-300-700 p-2 text-xs">
      <div class="mb-1 font-semibold">Preview</div>
      <div>target_prefix = <code>{zipPreview.target_prefix || '(root)'}</code></div>
      <div>
       files = {zipPreview.file_count}, conflicts = {zipPreview.conflicts.length}, danglings =
       {zipPreview.danglings.length}
      </div>
      {#if zipPreview.conflicts.length > 0}
       <details class="mt-1">
        <summary class="cursor-pointer text-warning-700-300">Conflicts</summary>
        <ul class="ml-4 list-disc">
         {#each zipPreview.conflicts as c (c)}
          <li><code>{c}</code></li>
         {/each}
        </ul>
       </details>
      {/if}
      {#if zipPreview.entries.length > 0}
       <details class="mt-1">
        <summary class="cursor-pointer">Entries ({zipPreview.entries.length})</summary>
        <ul class="ml-4 list-disc">
         {#each zipPreview.entries as e (e.zip_path)}
          <li>
           <code>{e.zip_path}</code> → <code>{e.dest_path}</code>
           <span class="opacity-60">({e.size}B)</span>
          </li>
         {/each}
        </ul>
       </details>
      {/if}
     </div>
    {/if}

    <div class="flex justify-end gap-2 pt-2">
     <button
      type="button"
      class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800"
      onclick={close}>Cancel</button
     >
     <button
      type="button"
      class="rounded bg-primary-500 px-3 py-1 text-xs font-semibold text-white hover:bg-primary-600 disabled:opacity-40"
      disabled={!zipFile || zipBusy || flowgraphStore.mutating}
      onclick={onImportApply}
     >
      {zipBusy ? 'Working…' : 'Import'}
     </button>
    </div>
   {/if}
  </div>
 </div>
{/if}
