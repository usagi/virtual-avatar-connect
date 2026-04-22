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
  */
 import { Handle, Position } from '@xyflow/svelte';
 import type { FlowgraphNodeSpec, FlowgraphPortSpec } from '../types';

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
  return 'flowgraph-handle data';
 }
</script>

<div
 class="flowgraph-node"
 class:selected
 class:missing-spec={!spec}
 title={data.feature}
>
 <div class="head">
  <div class="title">{title}</div>
  <div class="feature">{shortFeature(data.feature)}</div>
 </div>
 <div class="id">#{data.nodeId}</div>

 <div class="ports inputs">
  {#each inputs as p (p.name)}
   <div class="port-row">
    <Handle
     type="target"
     position={Position.Left}
     id={p.name}
     class={handleClass(p)}
     title={`${p.label} : ${p.ty}`}
    />
    <span class="label">{p.label}</span>
   </div>
  {/each}
 </div>
 <div class="ports outputs">
  {#each outputs as p (p.name)}
   <div class="port-row">
    <span class="label">{p.label}</span>
    <Handle
     type="source"
     position={Position.Right}
     id={p.name}
     class={handleClass(p)}
     title={`${p.label} : ${p.ty}`}
    />
   </div>
  {/each}
 </div>

 {#if !spec}
  <div class="warn">未知 feature</div>
 {/if}
</div>

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
