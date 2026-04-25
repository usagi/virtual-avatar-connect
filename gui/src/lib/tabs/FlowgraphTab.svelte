<script lang="ts">
 /**
  * Phase δ-6e: Flowgraph タブのトップレベル。
  *
  * レイアウト:
  *   ├── 上部: ツールバー（reload / save / open-external）
  *   ├── 中央:
  *   │    ├── 左サイドバー（ファイルツリー）
  *   │    ├── キャンバス（Svelte Flow）
  *   │    └── 右サイドバー（上: パレット / 下: プロパティエディタ）
  *   └── 下部: 診断パネル
  *
  * 責務:
  *   - マウント時に `flowgraphStore.refreshAll()` + WS 購読を開始
  *   - アンマウント時に WS 購読を解除
  *
  * 子コンポーネントとの通信は `flowgraphStore` 経由。props は最小に保つ。
  */
 import { onMount } from 'svelte';
 import { flowgraphStore, summarizeDiagnostics } from '../flowgraphStore.svelte';
 import FlowgraphTree from '../flowgraph/FlowgraphTree.svelte';
 import FlowgraphCanvas from '../flowgraph/FlowgraphCanvas.svelte';
 import FlowgraphPalette from '../flowgraph/FlowgraphPalette.svelte';
 import FlowgraphPropertyEditor from '../flowgraph/FlowgraphPropertyEditor.svelte';
 import FlowgraphDiagnostics from '../flowgraph/FlowgraphDiagnostics.svelte';
 import FlowgraphShareDialog from '../flowgraph/FlowgraphShareDialog.svelte';
 import { toastStore } from '../toasts.svelte';

let dialogOpen = $state(false);
let dialogMode = $state<'paste' | 'import_zip'>('paste');

onMount(() => {
  void flowgraphStore.refreshAll();
  flowgraphStore.attachWsSubscriber();
  // γ-4a: Ctrl+S / Cmd+S で現在ファイルを保存。フォーカスが input 系でも有効にするため window に付ける。
  const onKeyDown = (ev: KeyboardEvent) => {
   const t = ev.target as HTMLElement | null;
   if (t?.closest('input, textarea, select, [contenteditable="true"]')) return;

   const isSave = (ev.ctrlKey || ev.metaKey) && !ev.shiftKey && !ev.altKey && ev.key.toLowerCase() === 's';
   if (isSave) {
    if (!flowgraphStore.currentFq || flowgraphStore.mutating) return;
    ev.preventDefault();
    void flowgraphStore.saveCurrent();
    return;
   }

   const isDup =
    (ev.ctrlKey || ev.metaKey) && !ev.shiftKey && !ev.altKey && ev.key.toLowerCase() === 'd';
   if (isDup) {
    if (!flowgraphStore.currentFq || !flowgraphStore.selectedNodeId) return;
    ev.preventDefault();
    const ok = flowgraphStore.duplicateSelectedNode();
    if (ok) {
     toastStore.success('複製しました', flowgraphStore.selectedNodeId ?? '');
    }
    return;
   }
  };
  window.addEventListener('keydown', onKeyDown);
  // γ-4a: 未保存変更がある状態で閉じようとしたらブラウザにネイティブ確認ダイアログを出す。
  const onBeforeUnload = (ev: BeforeUnloadEvent) => {
   if (!flowgraphStore.isDirty) return;
   ev.preventDefault();
   ev.returnValue = '';
  };
  window.addEventListener('beforeunload', onBeforeUnload);
  return () => {
   flowgraphStore.detachWsSubscriber();
   window.removeEventListener('keydown', onKeyDown);
   window.removeEventListener('beforeunload', onBeforeUnload);
  };
 });

 const isDirty = $derived(flowgraphStore.isDirty);
 const saveLabel = $derived(
  flowgraphStore.mutating ? 'Saving…' : isDirty ? 'Save *' : 'Save',
 );

 const summary = $derived(summarizeDiagnostics(flowgraphStore.diagnostics?.diagnostics));
 const hasErrors = $derived(summary.error > 0);
 const hasWarnings = $derived(summary.warning > 0);

 async function onReload() {
  await flowgraphStore.reloadFromDisk();
 }

 async function onSave() {
  await flowgraphStore.saveCurrent();
 }

async function onOpenExternal() {
 if (flowgraphStore.currentFq) await flowgraphStore.openExternal(flowgraphStore.currentFq);
}

async function onCopy() {
 // 選択中ノードがあればそれを、なければ現在ファイル全体を fragment 化。
 if (flowgraphStore.selectedNodeId) await flowgraphStore.fragmentCopySelectedNode();
 else await flowgraphStore.fragmentCopyCurrentFile();
}

function onPaste() {
 dialogMode = 'paste';
 dialogOpen = true;
}

async function onExportZip() {
 if (!flowgraphStore.currentFq) return;
 const blob = await flowgraphStore.exportZip({
  scope: 'file',
  targets: [{ kind: 'file', fq: flowgraphStore.currentFq }],
  origin: flowgraphStore.currentFq,
 });
 if (!blob) return;
 const url = URL.createObjectURL(blob);
 const a = document.createElement('a');
 a.href = url;
 const safeName = flowgraphStore.currentFq.replace(/[\\/]/g, '_');
 a.download = `${safeName}.zip`;
 document.body.appendChild(a);
 a.click();
 a.remove();
 URL.revokeObjectURL(url);
}

function onImportZip() {
 dialogMode = 'import_zip';
 dialogOpen = true;
}
</script>

<div class="flex h-[calc(100vh-9rem)] flex-col gap-2">
 <!-- Toolbar -->
 <div class="flex flex-wrap items-center gap-2 rounded border border-surface-200-800 bg-surface-100-900 px-3 py-2">
  <div class="flex items-baseline gap-2">
   <span class="font-semibold">Flowgraph</span>
   <span class="text-xs opacity-60">
    root = <code>{flowgraphStore.tree?.root_dir ?? '(unknown)'}</code>
    {#if flowgraphStore.tree && !flowgraphStore.tree.exists}
     <span class="ml-1 rounded bg-warning-500/30 px-1 text-warning-900-100">missing</span>
    {/if}
   </span>
  </div>
  <div class="flex-1"></div>
  <span
   class="rounded px-2 py-0.5 text-xs"
   class:bg-error-500={hasErrors}
   class:text-white={hasErrors}
   class:bg-warning-500={!hasErrors && hasWarnings}
   class:bg-success-500={!hasErrors && !hasWarnings}
  >
   {summary.error}E / {summary.warning}W / {summary.info}I
  </span>
  <button
   type="button"
   class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800"
   title="ディスクから再ロード"
   onclick={onReload}
  >
   Reload
  </button>
  <button
   type="button"
   class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800 disabled:opacity-40"
   disabled={!flowgraphStore.currentFq}
   title="外部エディタで開く"
   onclick={onOpenExternal}
  >
   Open external
  </button>
 <button
  type="button"
  class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800 disabled:opacity-40"
  disabled={!flowgraphStore.currentFq}
  title="選択ノード、なければ現在ファイル全体を fragment TOML としてコピー"
  onclick={onCopy}
 >
  Copy
 </button>
 <button
  type="button"
  class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800"
  title="fragment TOML を paste"
  onclick={onPaste}
 >
  Paste…
 </button>
 <button
  type="button"
  class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800 disabled:opacity-40"
  disabled={!flowgraphStore.currentFq}
  title="現在ファイルを ZIP でエクスポート"
  onclick={onExportZip}
 >
  Export ZIP
 </button>
 <button
  type="button"
  class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800"
  title="ZIP を import（dry_run preview → 本番）"
  onclick={onImportZip}
 >
  Import ZIP…
 </button>
 <button
  type="button"
  class="rounded px-3 py-1 text-xs font-semibold text-white disabled:opacity-40"
  class:bg-primary-500={!isDirty}
  class:hover:bg-primary-600={!isDirty}
  class:bg-warning-500={isDirty}
  class:hover:bg-warning-600={isDirty}
  disabled={!flowgraphStore.currentFq || flowgraphStore.mutating}
  title={isDirty ? '未保存の変更があります（Ctrl+S）' : '現在のファイルを保存（Ctrl+S）'}
  onclick={onSave}
 >
  {saveLabel}
 </button>
</div>

<FlowgraphShareDialog bind:open={dialogOpen} bind:mode={dialogMode} />

 <!-- Main 3-pane -->
 <div class="grid flex-1 min-h-0 gap-2" style="grid-template-columns: 260px minmax(0, 1fr) 320px;">
  <div class="min-h-0 overflow-y-auto rounded border border-surface-200-800 bg-surface-50-950">
   <FlowgraphTree />
  </div>
  <div class="min-h-0 overflow-hidden rounded border border-surface-200-800 bg-surface-50-950">
   <FlowgraphCanvas />
  </div>
  <div class="flex min-h-0 flex-col gap-2">
   <div class="flex-1 min-h-0 overflow-y-auto rounded border border-surface-200-800 bg-surface-50-950">
    <FlowgraphPalette />
   </div>
   <div class="flex-1 min-h-0 overflow-y-auto rounded border border-surface-200-800 bg-surface-50-950">
    <FlowgraphPropertyEditor />
   </div>
  </div>
 </div>

 <!-- Diagnostics -->
 <div class="max-h-40 overflow-y-auto rounded border border-surface-200-800 bg-surface-50-950">
  <FlowgraphDiagnostics />
 </div>
</div>
