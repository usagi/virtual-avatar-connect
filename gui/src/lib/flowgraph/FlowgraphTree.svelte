<script lang="ts">
 /**
  * Phase δ-6e: ファイルツリー（左サイドバー）。
  *
  * - `flowgraphStore.tree.files` を平坦リストのまま fq の `/` 区切りで階層化して表示。
  * - 選択中ファイル（`store.currentFq`）はハイライト。
  * - 右クリック相当の UI は今フェーズでは実装せず、ヘッダにボタンを並べて賄う:
  *    "+ New"（fq を入力して新規）/ 選択中に対する Rename（δ-7）/ Delete / Open external。
  *    Rename は API として未提供なので今フェーズでは隠し、削除→新規作成で代替する方針（spec §8.6）。
  * - エラー（parse_error）はファイル行に赤マークで表示。
  */
 import { flowgraphStore } from '../flowgraphStore.svelte';
 import type { FlowgraphTreeFileEntry } from '../types';

 type TreeNode =
  | { kind: 'dir'; name: string; fullPath: string; children: TreeNode[] }
  | { kind: 'file'; name: string; fullPath: string; entry: FlowgraphTreeFileEntry };

 function buildTree(files: FlowgraphTreeFileEntry[]): TreeNode[] {
  const root: TreeNode[] = [];
  for (const f of files) {
   const parts = f.fq.split('/').filter(Boolean);
   if (parts.length === 0) continue;
   let here = root;
   let acc = '';
   for (let i = 0; i < parts.length; i++) {
    const name = parts[i];
    acc = acc ? `${acc}/${name}` : name;
    const isLast = i === parts.length - 1;
    if (isLast) {
     here.push({ kind: 'file', name, fullPath: acc, entry: f });
    } else {
     let dir = here.find(
      (c): c is Extract<TreeNode, { kind: 'dir' }> => c.kind === 'dir' && c.name === name,
     );
     if (!dir) {
      dir = { kind: 'dir', name, fullPath: acc, children: [] };
      here.push(dir);
     }
     here = dir.children;
    }
   }
  }
  sortRecursive(root);
  return root;
 }

 function sortRecursive(nodes: TreeNode[]) {
  nodes.sort((a, b) => {
   if (a.kind !== b.kind) return a.kind === 'dir' ? -1 : 1;
   return a.name.localeCompare(b.name);
  });
  for (const n of nodes) {
   if (n.kind === 'dir') sortRecursive(n.children);
  }
 }

 const tree = $derived(buildTree(flowgraphStore.tree?.files ?? []));

 async function onSelect(fq: string) {
  await flowgraphStore.openFile(fq);
 }

 async function onCreate() {
  const input = prompt('新規 fq（例: chat-echo/main）');
  if (!input) return;
  const fq = input.trim().replace(/\.flowgraph\.toml$/, '');
  if (!fq) return;
  await flowgraphStore.createFile(fq);
 }

 async function onDelete() {
  if (!flowgraphStore.currentFq) return;
  if (!confirm(`${flowgraphStore.currentFq} を削除しますか？（.bak に退避されます）`)) return;
  await flowgraphStore.deleteFile(flowgraphStore.currentFq);
 }
</script>

<div class="flex h-full flex-col">
 <div class="flex items-center justify-between border-b border-surface-200-800 p-2">
  <span class="text-xs font-semibold uppercase tracking-wider opacity-60">Files</span>
  <div class="flex gap-1">
   <button
    type="button"
    class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-200-800"
    title="新規 flowgraph ファイル"
    onclick={onCreate}
   >
    +
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-200-800 disabled:opacity-30"
    disabled={!flowgraphStore.currentFq}
    title="選択中ファイルを削除"
    onclick={onDelete}
   >
    ×
   </button>
  </div>
 </div>
 {#if flowgraphStore.treeState === 'loading'}
  <div class="p-3 text-xs opacity-60">Loading…</div>
 {:else if flowgraphStore.treeState === 'error'}
  <div class="p-3 text-xs text-error-500">{flowgraphStore.treeError}</div>
 {:else if flowgraphStore.tree && !flowgraphStore.tree.exists}
  <div class="p-3 text-xs opacity-70">
   <p class="mb-1">
    <code>{flowgraphStore.tree.root_dir}</code> が存在しません。
   </p>
   <p>新規ファイルを作成するとディレクトリごと作成されます。</p>
  </div>
 {:else if tree.length === 0}
  <div class="p-3 text-xs opacity-60">ファイルがありません。</div>
 {:else}
  <ul class="flex-1 overflow-y-auto p-1 text-sm">
   {#each tree as node (node.fullPath)}
    {@render treeItem(node, 0)}
   {/each}
  </ul>
 {/if}
</div>

{#snippet treeItem(node: TreeNode, depth: number)}
 {#if node.kind === 'dir'}
  <li style="padding-left: {depth * 0.75}rem">
   <div class="px-1 py-0.5 text-xs font-semibold opacity-70">{node.name}/</div>
   <ul>
    {#each node.children as child (child.fullPath)}
     {@render treeItem(child, depth + 1)}
    {/each}
   </ul>
  </li>
 {:else}
  {@const active = flowgraphStore.currentFq === node.entry.fq}
  {@const error = node.entry.parse_error}
  <li style="padding-left: {depth * 0.75}rem">
   <button
    type="button"
    class="flex w-full items-center justify-between gap-1 rounded px-1 py-0.5 text-left text-xs hover:bg-surface-200-800"
    class:bg-primary-500={active}
    class:text-white={active}
    onclick={() => onSelect(node.entry.fq)}
    title={node.entry.path}
   >
    <span class="truncate">{node.name}</span>
    <span class="flex items-center gap-1">
     {#if error}
      <span class="rounded bg-error-500 px-1 text-[0.65rem] text-white" title={error}>!</span>
     {:else if node.entry.node_count !== null}
      <span class="text-[0.65rem] opacity-60">{node.entry.node_count}</span>
     {/if}
    </span>
   </button>
  </li>
 {/if}
{/snippet}
