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
 let commandPaletteOpen = $state(false);
 let commandQuery = $state('');
 let filePaneWidth = $state(260);
 let inspectorPaneWidth = $state(320);
 let problemsHeight = $state(176);

type StudioCommand = {
 id: string;
 label: string;
 description: string;
 disabled: boolean;
 run: () => void | Promise<void>;
};

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

   const isUndo = (ev.ctrlKey || ev.metaKey) && !ev.altKey && ev.key.toLowerCase() === 'z';
   if (isUndo) {
    ev.preventDefault();
    if (ev.shiftKey) onRedo();
    else onUndo();
    return;
   }

   const isRedo = (ev.ctrlKey || ev.metaKey) && !ev.shiftKey && !ev.altKey && ev.key.toLowerCase() === 'y';
   if (isRedo) {
    ev.preventDefault();
    onRedo();
    return;
   }

   const isCommandPalette =
    (ev.ctrlKey || ev.metaKey) && !ev.shiftKey && !ev.altKey && ev.key.toLowerCase() === 'k';
   if (isCommandPalette) {
    ev.preventDefault();
    commandPaletteOpen = true;
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
 const fileCount = $derived(flowgraphStore.tree?.files.length ?? 0);
 const nodeCount = $derived(flowgraphStore.draftNodes?.length ?? 0);
 const edgeCount = $derived(flowgraphStore.draftEdges?.length ?? 0);
 const workspaceGridStyle = $derived(
  `grid-template-columns: ${filePaneWidth}px minmax(34rem, 1fr) ${inspectorPaneWidth}px;`,
 );
 const problemsStyle = $derived(`max-height: ${problemsHeight}px;`);
 const studioCommands: StudioCommand[] = $derived([
  {
   id: 'undo',
   label: 'Undo',
   description: 'Revert the last canvas edit.',
   disabled: !flowgraphStore.canUndo(),
   run: onUndo,
  },
  {
   id: 'redo',
   label: 'Redo',
   description: 'Reapply the last reverted canvas edit.',
   disabled: !flowgraphStore.canRedo(),
   run: onRedo,
  },
  {
   id: 'duplicate',
   label: 'Duplicate selection',
   description: 'Duplicate the selected node or node set.',
   disabled: !flowgraphStore.currentFq || flowgraphStore.selectedNodeIds.length === 0,
   run: onDuplicateSelection,
  },
  {
   id: 'align_horizontal',
   label: 'Align horizontal',
   description: 'Align selected nodes to the same y position.',
   disabled: flowgraphStore.selectedNodeIds.length < 2,
   run: () => flowgraphStore.alignSelectedNodes('y'),
  },
  {
   id: 'align_vertical',
   label: 'Align vertical',
   description: 'Align selected nodes to the same x position.',
   disabled: flowgraphStore.selectedNodeIds.length < 2,
   run: () => flowgraphStore.alignSelectedNodes('x'),
  },
  {
   id: 'distribute_horizontal',
   label: 'Distribute horizontal',
   description: 'Evenly space selected nodes along the x axis.',
   disabled: flowgraphStore.selectedNodeIds.length < 3,
   run: () => flowgraphStore.distributeSelectedNodes('x'),
  },
  {
   id: 'distribute_vertical',
   label: 'Distribute vertical',
   description: 'Evenly space selected nodes along the y axis.',
   disabled: flowgraphStore.selectedNodeIds.length < 3,
   run: () => flowgraphStore.distributeSelectedNodes('y'),
  },
  {
   id: 'group_layout',
   label: 'Create group',
   description: 'Save selected nodes as a visual Flowgraph group and pack them into a compact layout.',
   disabled: flowgraphStore.selectedNodeIds.length < 2,
   run: () => flowgraphStore.groupSelectedNodes(),
  },
  {
   id: 'select_group_members',
   label: 'Select group members',
   description: 'Select every node in the currently selected visual group.',
   disabled: !flowgraphStore.selectedGroupId,
   run: () => {
    if (flowgraphStore.selectedGroupId) flowgraphStore.selectGroupNodes(flowgraphStore.selectedGroupId);
   },
  },
  {
   id: 'remove_group',
   label: 'Remove selected group',
   description: 'Remove the selected visual group without deleting its member nodes.',
   disabled: !flowgraphStore.selectedGroupId,
   run: () => {
    if (flowgraphStore.selectedGroupId) flowgraphStore.removeGroup(flowgraphStore.selectedGroupId);
   },
  },
  {
   id: 'reload',
   label: 'Reload from disk',
   description: 'Refresh Flowgraph files and diagnostics from the configured root.',
   disabled: flowgraphStore.mutating,
   run: onReload,
  },
  {
   id: 'reload_preserve_state',
   label: 'Reload keeping live state',
   description: 'Save live state, then reload and restore from the profile-local snapshot file.',
   disabled: flowgraphStore.mutating,
   run: onReloadPreservingState,
  },
  {
   id: 'save',
   label: 'Save current file',
   description: 'Write the current Flowgraph file to disk.',
   disabled: !flowgraphStore.currentFq || flowgraphStore.mutating,
   run: onSave,
  },
  {
   id: 'open_external',
   label: 'Open external editor',
   description: 'Open the selected Flowgraph file in an external editor.',
   disabled: !flowgraphStore.currentFq,
   run: onOpenExternal,
  },
  {
   id: 'copy_fragment',
   label: 'Copy fragment',
   description: 'Copy the selected node, or the current file when no node is selected.',
   disabled: !flowgraphStore.currentFq,
   run: onCopy,
  },
  {
   id: 'paste_fragment',
   label: 'Paste fragment',
   description: 'Open the fragment paste dialog.',
   disabled: false,
   run: onPaste,
  },
  {
   id: 'export_zip',
   label: 'Export ZIP',
   description: 'Export the current Flowgraph file as a ZIP fragment.',
   disabled: !flowgraphStore.currentFq,
   run: onExportZip,
  },
  {
   id: 'import_zip',
   label: 'Import ZIP',
   description: 'Open the ZIP import preview dialog.',
   disabled: false,
   run: onImportZip,
  },
  ...nodeInsertCommands(),
 ]);

 const filteredStudioCommands = $derived.by(() => {
  const q = commandQuery.trim().toLowerCase();
  if (!q) return studioCommands;
  return studioCommands.filter((command) =>
   `${command.label} ${command.description}`.toLowerCase().includes(q),
  );
 });

 function nodeInsertCommands(): StudioCommand[] {
  const specs = flowgraphStore.catalog?.specs ?? [];
  return specs.slice(0, 250).map((spec) => ({
   id: `insert:${spec.feature}`,
   label: `Insert ${spec.title}`,
   description: `${spec.category} · ${spec.feature}`,
   disabled: !flowgraphStore.currentFq,
   run: () => {
    flowgraphStore.addCatalogNodeAt(spec, null);
   },
  }));
 }

 function onUndo() {
  if (flowgraphStore.undo()) toastStore.info('Undo', flowgraphStore.undoStack.at(-1)?.label ?? '');
 }

 function onRedo() {
  if (flowgraphStore.redo()) toastStore.info('Redo', flowgraphStore.redoStack.at(-1)?.label ?? '');
 }

 function onDuplicateSelection() {
  const ok = flowgraphStore.duplicateSelectedNode();
  if (ok) toastStore.success('Duplicated', `${flowgraphStore.selectedNodeIds.length} node(s)`);
 }

 async function onReload() {
  await flowgraphStore.reloadFromDisk();
 }

 async function onReloadPreservingState() {
  await flowgraphStore.reloadPreservingState();
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

 async function runStudioCommand(command: StudioCommand) {
  if (command.disabled) return;
  commandPaletteOpen = false;
  await command.run();
 }
</script>

<div class="flex h-[calc(100vh-9rem)] min-h-[42rem] flex-col gap-3">
 <div class="flex flex-wrap items-start justify-between gap-3">
  <div>
   <h2 class="text-xl font-semibold">Flowgraph Studio</h2>
   <p class="text-sm opacity-65">Author, inspect, and repair runtime dataflow files.</p>
  </div>
  <div class="vac-panel grid grid-cols-3 overflow-hidden text-center text-xs">
   <div class="border-r border-surface-200-800 px-3 py-2">
    <div class="font-semibold">{fileCount}</div>
    <div class="opacity-60">Files</div>
   </div>
   <div class="border-r border-surface-200-800 px-3 py-2">
    <div class="font-semibold">{nodeCount}</div>
    <div class="opacity-60">Nodes</div>
   </div>
   <div class="px-3 py-2">
    <div class="font-semibold">{edgeCount}</div>
    <div class="opacity-60">Edges</div>
   </div>
  </div>
 </div>

 <!-- Toolbar -->
 <div class="vac-panel-muted flex flex-wrap items-center gap-2 px-3 py-2">
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
  <button
   type="button"
   class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800"
   title="Open command palette (Ctrl+K)"
   onclick={() => (commandPaletteOpen = true)}
  >
   Commands
  </button>
  <span
   class="rounded px-2 py-0.5 text-xs"
   class:bg-error-500={hasErrors}
   class:text-white={hasErrors}
   class:bg-warning-500={!hasErrors && hasWarnings}
   class:bg-success-500={!hasErrors && !hasWarnings}
  >
   {summary.error}E / {summary.warning}W / {summary.info}I
  </span>
  <label class="flex items-center gap-1 text-[10px] opacity-70" title="Resize file pane">
   Files
   <input class="w-20" type="range" min="200" max="420" step="10" bind:value={filePaneWidth} />
  </label>
  <label class="flex items-center gap-1 text-[10px] opacity-70" title="Resize inspector pane">
   Side pane
   <input class="w-20" type="range" min="260" max="520" step="10" bind:value={inspectorPaneWidth} />
  </label>
  <label class="flex items-center gap-1 text-[10px] opacity-70" title="Resize problems panel">
   Bottom pane
   <input class="w-20" type="range" min="120" max="360" step="8" bind:value={problemsHeight} />
  </label>
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
   title="live state を保存してから再ロード"
   disabled={flowgraphStore.mutating}
   onclick={onReloadPreservingState}
  >
   Reload + state
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

{#if commandPaletteOpen}
 <div
  class="fixed inset-0 z-50 flex items-start justify-center bg-black/35 px-4 py-20"
  role="presentation"
  onclick={() => (commandPaletteOpen = false)}
 >
  <div
   class="vac-panel w-full max-w-xl overflow-hidden shadow-xl"
   role="dialog"
   aria-modal="true"
   aria-labelledby="flowgraph-command-palette-title"
   tabindex="-1"
   onkeydown={(ev) => {
    if (ev.key === 'Escape') commandPaletteOpen = false;
   }}
   onclick={(ev) => ev.stopPropagation()}
  >
   <div class="vac-panel-header flex items-center justify-between px-4 py-3">
    <div>
     <h3 id="flowgraph-command-palette-title" class="text-sm font-semibold">Command Palette</h3>
     <p class="text-xs opacity-60">Flowgraph Studio operations</p>
    </div>
    <button
     type="button"
     class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-200-800"
     onclick={() => (commandPaletteOpen = false)}
    >
     Close
    </button>
   </div>
   <div class="vac-panel-header px-3 py-2">
    <input
     class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-3 py-2 text-sm"
     placeholder="Search commands or node catalog"
     bind:value={commandQuery}
    />
   </div>
   <div class="grid gap-1 p-2">
    {#each filteredStudioCommands as command (command.id)}
     <button
      type="button"
       class="rounded px-3 py-2 text-left hover:bg-surface-100-900 disabled:opacity-40 disabled:hover:bg-transparent"
      disabled={command.disabled}
      onclick={() => void runStudioCommand(command)}
     >
      <div class="text-sm font-semibold">{command.label}</div>
      <div class="mt-0.5 text-xs opacity-60">{command.description}</div>
     </button>
    {:else}
     <div class="px-3 py-8 text-center text-sm opacity-60">No matching command.</div>
    {/each}
   </div>
  </div>
 </div>
{/if}

 <!-- Studio workspace -->
 <div class="grid min-h-0 flex-1 gap-2 xl:grid-cols-[260px_minmax(34rem,1fr)_320px]" style={workspaceGridStyle}>
  <section class="vac-panel flex min-h-[16rem] flex-col overflow-hidden">
   <div class="vac-panel-header px-3 py-2 text-xs font-semibold uppercase tracking-wider opacity-60">
    Files
   </div>
   <div class="min-h-0 flex-1 overflow-y-auto">
    <FlowgraphTree />
   </div>
  </section>

  <section class="vac-panel flex min-h-[28rem] flex-col overflow-hidden">
   <div class="vac-panel-header flex items-center justify-between gap-2 px-3 py-2">
    <span class="text-xs font-semibold uppercase tracking-wider opacity-60">Canvas</span>
    <span class="truncate text-xs opacity-60">{flowgraphStore.currentFq ?? 'No file selected'}</span>
   </div>
   <div class="min-h-0 flex-1 overflow-hidden">
    <FlowgraphCanvas />
   </div>
  </section>

  <div class="grid min-h-[28rem] grid-rows-[minmax(12rem,1fr)_minmax(12rem,1fr)] gap-2">
   <section class="vac-panel flex min-h-0 flex-col overflow-hidden">
    <div class="vac-panel-header px-3 py-2 text-xs font-semibold uppercase tracking-wider opacity-60">
     Node Palette
    </div>
    <div class="min-h-0 flex-1 overflow-hidden">
     <FlowgraphPalette />
    </div>
   </section>
   <section class="vac-panel flex min-h-0 flex-col overflow-hidden">
    <div class="vac-panel-header px-3 py-2 text-xs font-semibold uppercase tracking-wider opacity-60">
     Inspector
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto">
     <FlowgraphPropertyEditor />
    </div>
   </section>
  </div>
 </div>

 <!-- Problems -->
 <section class="vac-panel overflow-y-auto" style={problemsStyle}>
  <div class="vac-panel-header px-3 py-2 text-xs font-semibold uppercase tracking-wider opacity-60">
   Problems
  </div>
  <FlowgraphDiagnostics />
 </section>
</div>
