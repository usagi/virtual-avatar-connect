<script lang="ts">
 /**
  * Phase δ-6e: キャンバス（Svelte Flow で flowgraph の DAG を表示・編集）。
  *
  * 今フェーズの機能:
  *   - ノードの表示（title / feature / id / ポートを持つ簡易カード）
  *   - ドラッグで位置を編集 → `flowgraphStore.updateNodePosition` へ反映
  *   - エッジの表示（Bezier）
  *   - 新規エッジ接続（同一型 / 同一 is_exec のポート間に限定）
  *   - ノード選択 → `flowgraphStore.selectedNodeId` に反映（プロパティエディタが読む）
  *
  * 非スコープ:
  *   - Undo/Redo、マルチ選択、minimap、auto-layout（Phase ζ 以降）
  *
  * 設計メモ:
  *   - Svelte Flow の `nodes` / `edges` は $state な配列を要求するので、store からの変換は
  *     `$derived` ではなく `$effect` で store → local に push する（Svelte Flow 側でドラッグ
  *     されるとフィールドが直接書き換えられるので、変換結果は mutation-friendly にする必要がある）。
  *   - ドラッグ完了時（onnodedragstop）に store へ x/y を push。
  *   - 接続確立時（onconnect）に store.addEdge。
  *   - 削除は onnodesdelete / onedgesdelete でハンドル。
  */
 import {
  Background,
  Controls,
  MiniMap,
  SvelteFlow,
  type Connection,
  type Edge,
  type Node,
 } from '@xyflow/svelte';
 import '@xyflow/svelte/dist/style.css';
 import { flowgraphStore, parsePortRef } from '../flowgraphStore.svelte';
 import { toastStore } from '../toasts.svelte';
 import type { FlowgraphNodeSpec } from '../types';
 import FlowgraphNodeCard from './FlowgraphNodeCard.svelte';
 import FlowgraphGroupFrame from './FlowgraphGroupFrame.svelte';
 import FlowgraphAutoFit from './FlowgraphAutoFit.svelte';
 import FlowgraphPaneDropBridge from './FlowgraphPaneDropBridge.svelte';

 /** engine の `SocketType::compatible_with` + Phase λ `closed_string_variants` に概ね整合。 */
 function portsWireCompatible(
  outTy: string,
  inTy: string,
  outClosed?: string[] | null,
  inClosed?: string[] | null,
 ): boolean {
  let ok = false;
  if (outTy === inTy) ok = true;
  else if (outTy === 'json' || inTy === 'json') ok = true;
  else if ((outTy === 'float' && inTy === 'quantity') || (outTy === 'quantity' && inTy === 'float')) ok = true;
  else if (outTy === 'quantity' && inTy === 'string') ok = true;
  else if ((outTy === 'string' && inTy === 'datetime') || (outTy === 'datetime' && inTy === 'string')) ok = true;
  if (!ok) return false;
  if (outTy === 'string' && inTy === 'string' && inClosed?.length && outClosed?.length) {
   if (!outClosed.every((v) => inClosed.includes(v))) return false;
  }
  return true;
 }

 // Svelte Flow 用のデータ。store からの初期化 / 反映は $effect で同期する。
 let nodes = $state<Node[]>([]);
 let visibleNodes = $state<Node[]>([]);
 let edges = $state<Edge[]>([]);

 /** store の draft → Svelte Flow 用 Node[] への変換。 */
 function toFlowNodes(): Node[] {
  const draft = flowgraphStore.draftNodes ?? [];
  return draft.map((n, i) => {
   const spec = flowgraphStore.findSpec(n.feature);
   return {
    id: n.id,
    type: 'flowgraph',
    position: n.position
     ? { x: n.position[0], y: n.position[1] }
     : { x: 50 + (i % 6) * 220, y: 50 + Math.floor(i / 6) * 140 },
    data: {
     nodeId: n.id,
     feature: n.feature,
     spec,
     groupBadges: flowgraphStore.groupBadgesForNode(n.id),
    },
    selected: flowgraphStore.selectedNodeIds.includes(n.id) || flowgraphStore.selectedNodeId === n.id,
   };
  });
 }

 type GroupFrame = {
  id: string;
  label: string;
  nodeIds: string[];
  color: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
 };

 function toGroupFrameNodes(): Node[] {
  const draftNodes = flowgraphStore.draftNodes ?? [];
  const groups = flowgraphStore.draftGroups ?? [];
  if (draftNodes.length === 0 || groups.length === 0) return [];
  const posById = new Map(
   draftNodes.map((n, i) => [
    n.id,
    n.position ?? ([50 + (i % 6) * 220, 50 + Math.floor(i / 6) * 140] as [number, number]),
   ]),
  );
  const nodeWidth = 192;
  const nodeHeight = 108;
  const padding = 32;
  const frames: GroupFrame[] = groups.flatMap((g) => {
   const positions = g.node_ids
    .map((id) => posById.get(id))
    .filter((p): p is [number, number] => !!p);
   if (positions.length === 0) return [];
   const minX = Math.min(...positions.map((p) => p[0]));
   const minY = Math.min(...positions.map((p) => p[1]));
   const maxX = Math.max(...positions.map((p) => p[0] + nodeWidth));
   const maxY = Math.max(...positions.map((p) => p[1] + nodeHeight));
   return [
    {
     id: g.id,
     label: g.label || g.id,
     nodeIds: g.node_ids,
     color: g.color,
     x: Math.round(minX - padding),
     y: Math.round(minY - padding),
     width: Math.round(maxX - minX + padding * 2),
     height: Math.round(maxY - minY + padding * 2),
    },
   ];
  });
  return frames.map((g) => ({
   id: `__group:${g.id}`,
   type: 'flowgraphGroup',
   position: { x: g.x, y: g.y },
   data: {
    groupId: g.id,
    label: g.label,
    nodeCount: g.nodeIds.length,
    color: g.color,
    width: g.width,
    height: g.height,
   },
   selectable: false,
   draggable: false,
   deletable: false,
   focusable: false,
   zIndex: -10,
  }));
 }

 /** store の draft → Svelte Flow 用 Edge[]。 */
 function toFlowEdges(): Edge[] {
  const draftEdges = flowgraphStore.draftEdges ?? [];
  return draftEdges.map((e, i) => {
   const from = parsePortRef(e.from);
   const to = parsePortRef(e.to);
   const fromSpec = findPortSpec(from.nodeId, from.port, 'output');
   const toSpec = findPortSpec(to.nodeId, to.port, 'input');
   const isExec = fromSpec?.is_exec ?? false;
   const typeMismatch =
    fromSpec &&
    toSpec &&
    !isExec &&
    !portsWireCompatible(fromSpec.ty, toSpec.ty, fromSpec.closed_string_variants, toSpec.closed_string_variants);
   let style: string | undefined;
   if (isExec) style = 'stroke: rgb(249 115 22); stroke-width: 2;';
   else if (typeMismatch) style = 'stroke: rgb(239 68 68); stroke-width: 2; stroke-dasharray: 5 4;';
   return {
    id: `e-${i}-${e.from}->${e.to}`,
    source: from.nodeId,
    sourceHandle: from.port,
    target: to.nodeId,
    targetHandle: to.port,
    animated: isExec,
    data: { original: e },
    style,
   };
  });
 }

 function findPortSpec(
  nodeId: string,
  portName: string,
  dir: 'input' | 'output',
 ):
  | { is_exec: boolean; ty: string; closed_string_variants?: string[] | null }
  | undefined {
  const node = flowgraphStore.draftNodes?.find((n) => n.id === nodeId);
  if (!node) return undefined;
  const spec: FlowgraphNodeSpec | undefined = flowgraphStore.findSpec(node.feature);
  if (!spec) return undefined;
  const ports = dir === 'input' ? spec.inputs : spec.outputs;
  const p = ports.find((pp) => pp.name === portName);
  return p
   ? {
      is_exec: p.is_exec,
      ty: p.ty,
      closed_string_variants: p.closed_string_variants,
     }
   : undefined;
 }

 // store -> local sync
 $effect(() => {
  const nextNodes = toFlowNodes();
  nodes = nextNodes;
  visibleNodes = [...toGroupFrameNodes(), ...nextNodes];
  edges = toFlowEdges();
 });

 function onNodeDragStop(params: { targetNode: Node | null; nodes: Node[]; event: MouseEvent | TouchEvent }) {
  const n = params.targetNode;
  if (!n || isGroupFrameNode(n.id)) return;
  flowgraphStore.updateNodePosition(n.id, n.position.x, n.position.y);
 }

 function onConnect(c: Connection) {
  if (!c.source || !c.target) return;
  // 型チェック: 同じ is_exec 同士・同じ型同士のみ接続を許す。
  const outSpec = findPortSpec(c.source, c.sourceHandle ?? '', 'output');
  const inSpec = findPortSpec(c.target, c.targetHandle ?? '', 'input');
  if (!outSpec || !inSpec) return;
  if (outSpec.is_exec !== inSpec.is_exec) return;
  if (
   !outSpec.is_exec &&
   !portsWireCompatible(
    outSpec.ty,
    inSpec.ty,
    outSpec.closed_string_variants,
    inSpec.closed_string_variants,
   )
  )
   return;
  const from = `${c.source}:${c.sourceHandle ?? ''}`;
  const to = `${c.target}:${c.targetHandle ?? ''}`;
  flowgraphStore.addEdge(from, to);
 }

 function onDelete(params: { nodes: Node[]; edges: Edge[] }) {
  // γ-4a.0: まとめて削除し、直前スナップショットを store に保持して toast から undo できるようにする。
  const nodeIds = params.nodes.map((n) => n.id).filter((id) => !isGroupFrameNode(id));
  const edgePairs = params.edges
   .map((e) => (e.data as { original?: { from: string; to: string } } | undefined)?.original)
   .filter((x): x is { from: string; to: string } => !!x);
  const { removedNodes, removedEdges } = flowgraphStore.removeSelection(nodeIds, edgePairs);
  if (removedNodes === 0 && removedEdges === 0) return;
  const parts: string[] = [];
  if (removedNodes > 0) parts.push(`ノード ${removedNodes}`);
  if (removedEdges > 0) parts.push(`エッジ ${removedEdges}`);
  toastStore.info(
   '削除しました',
   parts.join(' / '),
   {
    label: '元に戻す',
    onAction: () => {
     flowgraphStore.undoLastDelete();
    },
   },
  );
 }

 function onSelectionChange(params: { nodes: Node[]; edges: Edge[] }) {
 const firstNode = params.nodes[0];
  flowgraphStore.selectedNodeIds = params.nodes.map((n) => n.id);
  flowgraphStore.selectedNodeId = firstNode ? firstNode.id : null;
 }

 function onNodeClick(params: { node: Node; event: MouseEvent | TouchEvent }) {
  const id = params.node.id;
  if (isGroupFrameNode(id)) return;
  const event = params.event;
  const additive = event instanceof MouseEvent && (event.ctrlKey || event.metaKey || event.shiftKey);
  if (!additive) {
   flowgraphStore.selectedNodeIds = [id];
   flowgraphStore.selectedNodeId = id;
   return;
  }
  const current = flowgraphStore.selectedNodeIds;
  const next = current.includes(id) ? current.filter((selected) => selected !== id) : [...current, id];
  flowgraphStore.selectedNodeIds = next;
  flowgraphStore.selectedNodeId = next[0] ?? null;
 }

 function isGroupFrameNode(id: string): boolean {
  return id.startsWith('__group:');
 }

 const nodeTypes = { flowgraph: FlowgraphNodeCard, flowgraphGroup: FlowgraphGroupFrame };
</script>

<div class="relative h-full w-full">
 {#if !flowgraphStore.currentFq}
  <div class="flex h-full items-center justify-center text-sm opacity-60">
   左からファイルを選択してください。
  </div>
 {:else if flowgraphStore.currentState === 'loading'}
  <div class="flex h-full items-center justify-center text-sm opacity-60">Loading…</div>
 {:else if flowgraphStore.currentState === 'error'}
  <div class="flex h-full items-center justify-center text-sm text-error-500">
   {flowgraphStore.currentError}
  </div>
 {:else if !flowgraphStore.currentFile?.parsed}
  <div class="flex h-full items-center justify-center px-4 text-center text-sm opacity-70">
   <div>
    <p class="mb-2 font-semibold text-error-500">TOML パース失敗</p>
    <p class="text-xs">
     外部エディタでファイルを修正してください。右上の「Open external」から開けます。
    </p>
   </div>
  </div>
 {:else}
  <SvelteFlow
   bind:nodes={visibleNodes}
   bind:edges
   {nodeTypes}
   fitView
   colorMode="system"
   multiSelectionKey={['Ctrl', 'Shift']}
   onnodeclick={onNodeClick}
   onconnect={onConnect}
   onnodedragstop={onNodeDragStop}
   ondelete={onDelete}
   onselectionchange={onSelectionChange}
  >
   <FlowgraphPaneDropBridge />
   <Background />
   <Controls />
   <MiniMap pannable zoomable />
   <!-- ファイル切替 or ノード数変化のたびに 2 フレーム待ち fitView を発火する。
        `<SvelteFlow fitView />` の既定は初回マウント時のみ fit するため、
        $effect で後から nodes を差し込む我々の構成では明示的な再 fit が必須。 -->
   <FlowgraphAutoFit triggerKey={`${flowgraphStore.currentFq ?? ''}::${nodes.length}`} />
  </SvelteFlow>
  <!-- γ-4a.0: 削除操作の発見性確保。編集中キャンバスの左下にキーボードヒントを固定表示。 -->
  <div class="flowgraph-keyhint pointer-events-none absolute bottom-1 left-1 select-none">
   <code>Delete</code> / <code>Backspace</code> : 選択削除 ・ <code>Ctrl+D</code> : 複製 ・ <code>Ctrl+Click</code> : 複数選択
  </div>
 {/if}
</div>

<style>
 /* Svelte Flow のキャンバスは親 (tab) の dark/light 背景をそのまま使う。
    colorMode="system" を SvelteFlow に渡しているので xyflow 側の内部テーマは
    OS の prefers-color-scheme に連動する。ここで background: transparent
    にしておくと skeleton 側の surface 色が透けて、上位タブと一体感が出る。 */
 :global(.svelte-flow) {
  background: transparent;
 }
 /* OS dark モードでは xyflow の pane / viewport の背景色が設定されていても
    全体としては skeleton の dark 面に乗せるのが見やすい。`.svelte-flow__pane`
    は xyflow 内部クラスだが、style.css が上書きしてくる場合に備えて透明化。 */
 :global(.dark .svelte-flow__pane),
 :global([data-color-mode='dark'] .svelte-flow__pane) {
  background: transparent;
 }
 /* γ-4a.0: キーボードヒント。控えめな配色で、操作の邪魔にならないサイズ感。 */
 .flowgraph-keyhint {
  font-size: 10.5px;
  opacity: 0.55;
  background: rgba(0, 0, 0, 0.04);
  border-radius: 4px;
  padding: 2px 6px;
 }
 .flowgraph-keyhint code {
  font-family: inherit;
  font-weight: 600;
  padding: 0 3px;
  border: 1px solid currentColor;
  border-radius: 3px;
  opacity: 0.9;
 }
 @media (prefers-color-scheme: dark) {
  .flowgraph-keyhint {
   background: rgba(255, 255, 255, 0.06);
  }
 }
</style>
