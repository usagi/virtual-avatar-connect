<script lang="ts">
 /**
  * Phase ο-6: HTML5 DnD の drop を flow 座標に変換して `flowgraphStore.addCatalogNodeAt` へ渡す。
  * `useSvelteFlow()` 必須のため **SvelteFlow の直下子**としてだけマウントする。
  */
 import { useSvelteFlow } from '@xyflow/svelte';
 import { flowgraphStore } from '../flowgraphStore.svelte';

 const { screenToFlowPosition } = useSvelteFlow();
 let anchor = $state<HTMLDivElement | null>(null);

 $effect(() => {
  const a = anchor;
  if (!a) return;
  const root = a.closest('.svelte-flow');
  const pane = root?.querySelector('.svelte-flow__pane');
  if (!(pane instanceof HTMLElement)) return;

  const onDragOver = (e: DragEvent) => {
   e.preventDefault();
   if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
  };
  const onDrop = (e: DragEvent) => {
   e.preventDefault();
   const feature =
    e.dataTransfer?.getData('application/x-vac-flowgraph-feature')?.trim() ||
    e.dataTransfer?.getData('text/plain')?.trim();
   if (!feature || !feature.startsWith('flowgraph.')) return;
   const spec = flowgraphStore.findSpec(feature);
   if (!spec) return;
   const p = screenToFlowPosition({ x: e.clientX, y: e.clientY });
   flowgraphStore.addCatalogNodeAt(spec, [p.x, p.y]);
  };

  pane.addEventListener('dragover', onDragOver);
  pane.addEventListener('drop', onDrop as EventListener);
  return () => {
   pane.removeEventListener('dragover', onDragOver);
   pane.removeEventListener('drop', onDrop as EventListener);
  };
 });
</script>

<div bind:this={anchor} class="flowgraph-dnd-anchor" aria-hidden="true"></div>

<style>
 .flowgraph-dnd-anchor {
  position: absolute;
  left: 0;
  top: 0;
  width: 0;
  height: 0;
  overflow: hidden;
  pointer-events: none;
  visibility: hidden;
 }
</style>
