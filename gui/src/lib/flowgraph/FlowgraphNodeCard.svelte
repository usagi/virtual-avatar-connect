<script lang="ts">
 /**
  * Svelte Flow のカスタムノード。flowgraph のノードカード。
  *
  * - 左側: 入力ポート（<Handle type="target">）縦並び
  * - 右側: 出力ポート（<Handle type="source">）縦並び
  * - 中央: title / feature 短縮形 / id
  * - is_exec ポートは矢印形をイメージしたアクセント色で描画
  *
  * 接続時の型チェックは `FlowgraphCanvas` の onConnect で行なうのでここは表示のみ。
  *
  * Phase φ-6: `spec.control_triggerable === true` のノードには ▶ Trigger ボタンを出す。
  * クリックで `FlowgraphTriggerDialog` を開き、Control API から 1-shot 発火できる。
  */
 import { Handle, Position } from '@xyflow/svelte';
 import { flowgraphStore } from '../flowgraphStore.svelte';
 import { quantityFamilyClassFromDim, quantityPortTooltip } from '../quantityDisplay';
 import { toastStore } from '../toasts.svelte';
 import type { FlowgraphNodeSpec, FlowgraphPortSpec } from '../types';
 import FlowgraphTriggerDialog from './FlowgraphTriggerDialog.svelte';

 type Data = {
  nodeId: string;
  feature: string;
  spec: FlowgraphNodeSpec | undefined;
 };

 let { data, selected }: { data: Data; selected?: boolean } = $props();

 function shortFeature(feature: string): string {
  const parts = feature.split('.');
  if (parts.length <= 1) return feature;
  return parts[parts.length - 1];
 }

 const spec = $derived(data.spec);
 const title = $derived(spec?.title ?? data.feature);
 const inputs: FlowgraphPortSpec[] = $derived(
  (spec?.inputs ?? []).filter((p) => !p.name.startsWith('__') || !p.name.endsWith('__')),
 );
 const outputs: FlowgraphPortSpec[] = $derived(
  (spec?.outputs ?? []).filter((p) => !p.name.startsWith('__') || !p.name.endsWith('__')),
 );

 function handleClass(port: FlowgraphPortSpec): string {
  if (port.is_exec) return 'flowgraph-handle exec';
  // η: Table ポートは辞書/表データを表す特別なハンドルとして視覚的に区別する。
  if (port.ty === 'table') return 'flowgraph-handle data table';
  // ξ-5: Quantity は SI 単位付き数値。次元 family は port-row の class で色分け。
  if (port.ty === 'quantity') return 'flowgraph-handle data quantity';
  return 'flowgraph-handle data';
 }

 function portRowClass(port: FlowgraphPortSpec): string {
  const base = 'port-row';
  if (port.ty !== 'quantity') return base;
  return `${base} ${quantityFamilyClassFromDim(port.quantity_dim)}`;
 }

 /** γ-4a.0: × ボタンから単一ノード削除。FlowgraphCanvas の onDelete と同じ挙動（undo toast 付き）。 */
 function onClickDelete(event: MouseEvent) {
  event.stopPropagation();
  event.preventDefault();
  const { removedNodes, removedEdges } = flowgraphStore.removeSelection([data.nodeId], []);
  if (removedNodes === 0 && removedEdges === 0) return;
  const parts: string[] = [];
  if (removedNodes > 0) parts.push(`ノード ${removedNodes}`);
  if (removedEdges > 0) parts.push(`エッジ ${removedEdges}`);
  toastStore.info('削除しました', parts.join(' / '), {
   label: '元に戻す',
   onAction: () => {
    flowgraphStore.undoLastDelete();
   },
  });
 }

 // φ-6: Trigger ボタン。control_triggerable=true のノードにだけ出す。
 const triggerable = $derived(spec?.control_triggerable === true);
 /**
  * サーバの `node_meta` キー（= `{fq_path}::{node_id}`）に揃えるため、現在開いている
  * flowgraph の fq と node id を連結する。currentFq が空（root 直下）の時は id 単体。
  */
 const fqNodeId = $derived.by(() => {
  const fq = flowgraphStore.currentFq;
  if (!fq || fq.length === 0) return data.nodeId;
  return `${fq}::${data.nodeId}`;
 });
 let triggerDialogOpen = $state(false);

 function onClickTrigger(event: MouseEvent) {
  event.stopPropagation();
  event.preventDefault();
  if (!spec) return;
  triggerDialogOpen = true;
 }

 function onMouseDownNode(event: MouseEvent) {
  const additive = event.ctrlKey || event.metaKey || event.shiftKey;
  if (!additive) return;
  event.stopPropagation();
  event.preventDefault();
  const current = flowgraphStore.selectedNodeIds;
  const next = current.includes(data.nodeId)
   ? current.filter((selected) => selected !== data.nodeId)
   : [...current, data.nodeId];
  flowgraphStore.selectedNodeIds = next;
  flowgraphStore.selectedNodeId = next[0] ?? null;
 }
</script>

<div
 class="flowgraph-node"
 class:selected
 class:missing-spec={!spec}
 title={data.feature}
 data-testid={`flowgraph-node-${data.nodeId}`}
 role="button"
 tabindex="0"
 aria-label={`Flowgraph node ${data.nodeId}`}
 onmousedown={onMouseDownNode}
>
 <button
  type="button"
  class="close-btn"
  aria-label="ノード削除"
  title="このノードを削除 (Delete でも可)"
  onclick={onClickDelete}
  onmousedown={(e) => e.stopPropagation()}
 >
  ×
 </button>
 {#if triggerable && spec}
  <button
   type="button"
   class="trigger-btn"
   aria-label="このノードをトリガ"
   title="Control API からこのノードを 1-shot で発火する (φ-6)"
   onclick={onClickTrigger}
   onmousedown={(e) => e.stopPropagation()}
  >
   ▶
  </button>
 {/if}
 <div class="head">
  <div class="title">{title}</div>
  <div class="feature">{shortFeature(data.feature)}</div>
 </div>
 <div class="id">#{data.nodeId}</div>

 <div class="ports inputs">
  {#each inputs as p (p.name)}
   <div class={portRowClass(p)}>
    <Handle
     type="target"
     position={Position.Left}
     id={p.name}
     class={handleClass(p)}
     title={quantityPortTooltip(p)}
    />
    <span class="label">{p.label}</span>
    {#if p.ty === 'quantity' && p.quantity_unit_badge}
     <span class="unit-badge" data-testid="flowgraph-quantity-badge">{p.quantity_unit_badge}</span>
    {/if}
   </div>
  {/each}
 </div>
 <div class="ports outputs">
  {#each outputs as p (p.name)}
   <div class={portRowClass(p)}>
    <span class="label">{p.label}</span>
    {#if p.ty === 'quantity' && p.quantity_unit_badge}
     <span class="unit-badge" data-testid="flowgraph-quantity-badge">{p.quantity_unit_badge}</span>
    {/if}
    <Handle
     type="source"
     position={Position.Right}
     id={p.name}
     class={handleClass(p)}
     title={quantityPortTooltip(p)}
    />
   </div>
  {/each}
 </div>

 {#if !spec}
  <div class="warn">未知 feature</div>
 {/if}
</div>

{#if triggerable && spec}
 <FlowgraphTriggerDialog
  bind:open={triggerDialogOpen}
  {fqNodeId}
  {spec}
 />
{/if}

<style>
 .flowgraph-node {
  position: relative;
  min-width: 220px;
  max-width: 300px;
  border: 1px solid rgba(120, 120, 140, 0.6);
  border-radius: 6px;
  background: var(--color-surface-50, #ffffff);
  color: var(--color-surface-950, #111);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
  padding: 3px 4px 5px 4px;
  font-size: 11.5px;
  line-height: 1.25;
  /* 入力列 / 出力列をグリッドで左右に並べて縦サイズを圧縮する。
     従来は head → id → inputs (縦積み) → outputs (縦積み) の順で全部縦に積まれていたため、
     ポート数の多いノード (例: Screenshot Capture = 7 入力 + 7 出力) が 200px 以上に伸びて
     隣接ノードと重なる原因になっていた。grid-template-areas で inputs/outputs を同じ行に
     置けば、ノードの縦サイズは max(inputs, outputs) に縮む。*/
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-template-areas:
    'head head'
    'id id'
    'inputs outputs'
    'warn warn';
  column-gap: 6px;
  row-gap: 0;
 }
 .flowgraph-node.selected {
  border-color: rgb(59 130 246);
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.35);
 }
 /* γ-4a.0: hover 時のみ × ボタン表示。選択状態は常時表示。 */
 .close-btn {
  position: absolute;
  top: 2px;
  right: 3px;
  width: 16px;
  height: 16px;
  padding: 0;
  line-height: 14px;
  font-size: 14px;
  font-weight: 600;
  border: 1px solid rgba(239, 68, 68, 0.45);
  color: rgb(185, 28, 28);
  background: rgba(239, 68, 68, 0.1);
  border-radius: 3px;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.12s ease-in-out, background-color 0.12s ease-in-out;
  z-index: 5;
 }
 .flowgraph-node:hover .close-btn,
 .flowgraph-node.selected .close-btn {
  opacity: 0.85;
 }
 .close-btn:hover {
  opacity: 1;
  background: rgba(239, 68, 68, 0.25);
 }
 /* φ-6: Trigger ボタン。control_triggerable=true のノードにだけ出る。
    × ボタンの左隣に固定配置し、hover / selected で可視化、通常時は半透明で邪魔にならないように。 */
 .trigger-btn {
  position: absolute;
  top: 2px;
  right: 22px;
  width: 16px;
  height: 16px;
  padding: 0;
  line-height: 14px;
  font-size: 11px;
  font-weight: 700;
  border: 1px solid rgba(59, 130, 246, 0.5);
  color: rgb(29, 78, 216);
  background: rgba(59, 130, 246, 0.1);
  border-radius: 3px;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.12s ease-in-out, background-color 0.12s ease-in-out;
  z-index: 5;
 }
 .flowgraph-node:hover .trigger-btn,
 .flowgraph-node.selected .trigger-btn {
  opacity: 0.9;
 }
 .trigger-btn:hover {
  opacity: 1;
  background: rgba(59, 130, 246, 0.25);
 }
 .flowgraph-node.missing-spec {
  border-color: rgb(239 68 68);
 }
 .head {
  grid-area: head;
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 6px;
  padding: 0 8px 0;
 }
 .title {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
 }
 .feature {
  font-size: 10px;
  opacity: 0.55;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
 }
 .id {
  grid-area: id;
  padding: 0 8px 3px;
  font-size: 10px;
  opacity: 0.55;
 }
 .ports {
  display: flex;
  flex-direction: column;
  gap: 1px;
 }
 .ports.inputs {
  grid-area: inputs;
 }
 .ports.outputs {
  grid-area: outputs;
 }
 .port-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  /* Svelte Flow の <Handle> が position:absolute で top:50% に並ぶため、
     Handle をこの port-row に対して相対配置できるよう relative を付ける。
     これが無いとすべての Handle がノード全体の縦中央 (y=50%) に積み重なって
     edge が視覚的にノード中央から出入りしてしまう（δ-9 Part E 見つけ）。*/
  position: relative;
  min-height: 12px;
 }
 .ports.outputs .port-row {
  justify-content: flex-end;
  flex-direction: row;
 }
 .label {
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  flex: 1 1 auto;
 }
 /* ξ-5: default から復元できた単位のみ短縮表示 */
 .unit-badge {
  flex: 0 0 auto;
  max-width: 4.5rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 9px;
  font-weight: 600;
  padding: 0 3px;
  border-radius: 3px;
  background: rgba(139, 92, 246, 0.2);
  color: rgb(91, 33, 182);
 }
 /* ==== Handle の絶対配置を port-row 単位に縛る ==== */
 .ports.inputs :global(.flowgraph-handle) {
  left: -6px !important;
  top: 50% !important;
  right: auto !important;
  transform: translateY(-50%) !important;
 }
 .ports.outputs :global(.flowgraph-handle) {
  left: auto !important;
  right: -6px !important;
  top: 50% !important;
  transform: translateY(-50%) !important;
 }
 /* exec（三角）側は border トリックでサイズを作っているので、clickable 領域がほぼ
    無い。pointer 用に多めの外周 hitbox を確保する目的で ::before を使ってもよいが、
    まずは視覚位置の修正だけ入れる。*/
 .warn {
  grid-area: warn;
  margin: 6px 8px 0;
  padding: 2px 6px;
  border-radius: 4px;
  background: rgba(239, 68, 68, 0.2);
  color: rgb(153, 27, 27);
  font-size: 10px;
 }
 :global(.flowgraph-handle.data) {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: rgb(59 130 246);
  border: 2px solid white;
 }
 /* η: Table 型のポートは角を落とした正方形 + 深緑で他のデータと視覚的に区別する。 */
 :global(.flowgraph-handle.data.table) {
  width: 12px;
  height: 12px;
  border-radius: 3px;
  background: rgb(16 185 129);
  border: 2px solid white;
 }
 /* ξ-5: Quantity 既定色（family 無指定）。port-row の family class で上書き。 */
 :global(.flowgraph-handle.data.quantity) {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: rgb(139 92 246);
  border: 2px solid white;
 }
 .port-row.qty-family-length :global(.flowgraph-handle.data.quantity) {
  background: rgb(34 197 94);
 }
 .port-row.qty-family-mass :global(.flowgraph-handle.data.quantity) {
  background: rgb(168 85 247);
 }
 .port-row.qty-family-time :global(.flowgraph-handle.data.quantity) {
  background: rgb(59 130 246);
 }
 .port-row.qty-family-current :global(.flowgraph-handle.data.quantity) {
  background: rgb(249 115 22);
 }
 .port-row.qty-family-temperature :global(.flowgraph-handle.data.quantity) {
  background: rgb(244 63 94);
 }
 .port-row.qty-family-amount :global(.flowgraph-handle.data.quantity) {
  background: rgb(6 182 212);
 }
 .port-row.qty-family-luminous :global(.flowgraph-handle.data.quantity) {
  background: rgb(234 179 8);
 }
 .port-row.qty-family-angle :global(.flowgraph-handle.data.quantity) {
  background: rgb(202 138 4);
 }
 .port-row.qty-family-mixed :global(.flowgraph-handle.data.quantity) {
  background: rgb(99 102 241);
 }
 :global(.flowgraph-handle.exec) {
  width: 0;
  height: 0;
  border-top: 7px solid transparent;
  border-bottom: 7px solid transparent;
  background: transparent;
  border-radius: 0;
  /* 左向き (target) / 右向き (source) は Svelte Flow 側の positioning で揃う。 */
  border-left: 10px solid rgb(249 115 22);
 }
 @media (prefers-color-scheme: dark) {
  .flowgraph-node {
   background: var(--color-surface-900, #1b1b1f);
   color: var(--color-surface-50, #f0f0f0);
   border-color: rgba(200, 200, 220, 0.25);
  }
 }
</style>
