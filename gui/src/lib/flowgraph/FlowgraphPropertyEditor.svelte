<script lang="ts">
 /**
  * Phase δ-6e: プロパティエディタ（右下）。
  *
  * - `flowgraphStore.selectedNodeId` で選択されたノードの properties を型別に編集する。
  * - 型: bool / int / float / string / json / list<T> / map<T>。
  *   - json / list / map / 複合プロパティは textarea で JSON 文字列として編集する（MVP）。
  *   - bool は checkbox、数値は number input、文字列は text input。
  * - required 未指定のプロパティは "Default" バッジ + "Set" ボタンで明示的に追加する UI。
  * - 不明プロパティ（spec 側に無い）は "Unknown" バッジ + 削除ボタン。
  */
 import { api } from '../api';
 import { flowgraphStore, type FlowgraphDraftGroup, type FlowgraphDraftNode } from '../flowgraphStore.svelte';
 import type { FlowgraphNodeSpec, FlowgraphPropertySpec } from '../types';

 const UNIT_PARSE_PROP_NAMES = new Set(['unit', 'target_unit', 'unit_override']);

 function isUnitStringProp(p: FlowgraphPropertySpec): boolean {
  return p.ty === 'string' && UNIT_PARSE_PROP_NAMES.has(p.name);
 }

 /** propName → parse エラーメッセージ（valid 時はキー無し） */
let unitParseByProp = $state<Record<string, string>>({});
// Timer handles are imperative bookkeeping, not UI state.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const unitParseTimers = new Map<string, ReturnType<typeof setTimeout>>();

 let prevSelectedNodeId = $state<string | null>(null);
 $effect(() => {
  const id = flowgraphStore.selectedNodeId;
  if (id !== prevSelectedNodeId) {
   prevSelectedNodeId = id;
   unitParseByProp = {};
  }
 });

 function scheduleUnitParse(propName: string, raw: string) {
  clearTimeout(unitParseTimers.get(propName));
  unitParseTimers.set(
   propName,
   setTimeout(() => {
    void (async () => {
     try {
      const r = await api.flowgraphParseUnit(raw);
      if (r.valid) {
       const next = { ...unitParseByProp };
       delete next[propName];
       unitParseByProp = next;
      } else {
       unitParseByProp = {
        ...unitParseByProp,
        [propName]: r.error ?? '単位として解釈できません',
       };
      }
     } catch (e) {
      unitParseByProp = {
       ...unitParseByProp,
       [propName]: e instanceof Error ? e.message : String(e),
      };
     }
    })();
   }, 380),
  );
 }

 const node: FlowgraphDraftNode | undefined = $derived(
  flowgraphStore.selectedNodeId
   ? flowgraphStore.draftNodes?.find((n) => n.id === flowgraphStore.selectedNodeId)
   : undefined,
 );
 const selectedNodes: FlowgraphDraftNode[] = $derived.by(() => {
  const ids = flowgraphStore.selectedNodeIds;
  const nodes = flowgraphStore.draftNodes ?? [];
  if (ids.length === 0) return node ? [node] : [];
  const selected = new Set(ids);
  return nodes.filter((n) => selected.has(n.id));
 });
 const spec: FlowgraphNodeSpec | undefined = $derived(
  node ? flowgraphStore.findSpec(node.feature) : undefined,
 );
 const multiSpec: FlowgraphNodeSpec | undefined = $derived.by(() => {
  if (selectedNodes.length < 2) return undefined;
  const feature = selectedNodes[0]?.feature;
  if (!feature || !selectedNodes.every((n) => n.feature === feature)) return undefined;
  return flowgraphStore.findSpec(feature);
 });

 function updateProp(key: string, value: unknown) {
  if (!node) return;
  flowgraphStore.updateNodeProperty(node.id, key, value);
 }

 function updateMultiProp(key: string, value: unknown) {
  if (selectedNodes.length < 2) return;
  flowgraphStore.updateNodePropertyMany(selectedNodes.map((n) => n.id), key, value);
 }

 function removeProp(key: string) {
  if (!node) return;
  flowgraphStore.removeNodeProperty(node.id, key);
 }

 function removeMultiProp(key: string) {
  if (selectedNodes.length < 2) return;
  flowgraphStore.removeNodePropertyMany(selectedNodes.map((n) => n.id), key);
 }

 function setPropToDefault(p: FlowgraphPropertySpec) {
  if (!node) return;
  flowgraphStore.updateNodeProperty(node.id, p.name, p.default);
 }

 function setMultiPropToDefault(p: FlowgraphPropertySpec) {
  if (selectedNodes.length < 2) return;
  flowgraphStore.updateNodePropertyMany(selectedNodes.map((n) => n.id), p.name, p.default);
 }

 function onJsonChange(key: string, text: string) {
  try {
   const parsed = JSON.parse(text);
   updateProp(key, parsed);
  } catch {
   // パース失敗時は update を保留する（GUI 上の文字列だけ残す）
  }
 }

 function onMultiJsonChange(key: string, text: string) {
  try {
   const parsed = JSON.parse(text);
   updateMultiProp(key, parsed);
  } catch {
   // パース失敗時は update を保留する。
  }
 }

 function typeCategory(
  ty: string,
 ): 'bool' | 'int' | 'float' | 'string' | 'json' | 'list' | 'map' {
  if (ty === 'bool') return 'bool';
  if (ty === 'int') return 'int';
  if (ty === 'float') return 'float';
  if (ty === 'string') return 'string';
  if (ty === 'json') return 'json';
  if (ty.startsWith('list<')) return 'list';
  if (ty.startsWith('map<')) return 'map';
  return 'json';
 }

 function asString(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return v;
  return JSON.stringify(v);
 }

 function asNumber(v: unknown): number {
  if (typeof v === 'number') return v;
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
 }

 function asBool(v: unknown): boolean {
  return v === true;
 }

 function asJson(v: unknown): string {
  try {
   return JSON.stringify(v, null, 2);
  } catch {
   return String(v);
  }
 }

 function knownPropsAndUnknowns(): {
  known: Array<{ p: FlowgraphPropertySpec; present: boolean; value: unknown }>;
  unknown: Array<{ name: string; value: unknown }>;
 } {
  if (!node) return { known: [], unknown: [] };
  const known: Array<{ p: FlowgraphPropertySpec; present: boolean; value: unknown }> = [];
  const consumed: Record<string, true> = Object.create(null);
  for (const p of spec?.properties ?? []) {
   const present = Object.prototype.hasOwnProperty.call(node.properties, p.name);
   known.push({ p, present, value: present ? node.properties[p.name] : p.default });
   consumed[p.name] = true;
  }
  const unknown: Array<{ name: string; value: unknown }> = [];
  for (const [k, v] of Object.entries(node.properties)) {
   if (!consumed[k]) unknown.push({ name: k, value: v });
  }
  return { known, unknown };
 }

 const propsView = $derived(knownPropsAndUnknowns());

 function valuesEqual(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
 }

 function multiPropsView(): Array<{
  p: FlowgraphPropertySpec;
  presentCount: number;
  value: unknown;
  mixed: boolean;
 }> {
  if (!multiSpec || selectedNodes.length < 2) return [];
  return multiSpec.properties.map((p) => {
   const values = selectedNodes
    .filter((n) => Object.prototype.hasOwnProperty.call(n.properties, p.name))
    .map((n) => n.properties[p.name]);
   const presentCount = values.length;
   const first = presentCount > 0 ? values[0] : p.default;
   const mixed = presentCount > 0 && !values.every((v) => valuesEqual(v, first));
   return { p, presentCount, value: mixed ? p.default : first, mixed };
  });
 }

 const multiProps = $derived(multiPropsView());

 const visibleGroups: FlowgraphDraftGroup[] = $derived.by(() => {
  const groups = flowgraphStore.draftGroups ?? [];
  if (groups.length === 0) return [];
  const selectedIds = new Set(selectedNodes.map((n) => n.id));
  if (selectedIds.size === 0) return groups;
  return groups.filter((g) => g.node_ids.some((id) => selectedIds.has(id)));
 });

 function updateGroupLabel(id: string, value: string) {
  flowgraphStore.updateGroupLabel(id, value);
 }

 function updateGroupColor(id: string, value: string) {
  flowgraphStore.updateGroupColor(id, value);
 }
</script>

<div class="flex h-full flex-col">
 <div class="border-b border-surface-200-800 p-2 text-xs font-semibold uppercase tracking-wider opacity-60">
  Properties
 </div>
 {#if selectedNodes.length > 1}
  <div class="flex-1 overflow-y-auto p-2 text-xs">
   <div class="mb-2 rounded bg-surface-100-900 p-2">
    <div class="mb-1 flex items-baseline justify-between gap-1">
     <span class="truncate font-semibold">{selectedNodes.length} nodes selected</span>
     {#if multiSpec}
      <span class="truncate text-[0.65rem] opacity-60">{multiSpec.feature}</span>
     {/if}
    </div>
    <div class="space-y-0.5 font-mono text-[0.65rem] opacity-70">
     {#each selectedNodes.slice(0, 8) as n (n.id)}
      <div class="truncate">#{n.id} · {n.feature}</div>
     {/each}
     {#if selectedNodes.length > 8}
      <div>+{selectedNodes.length - 8}</div>
     {/if}
    </div>
   </div>

   {#if !multiSpec}
    <div class="rounded border border-surface-200-800 p-2 opacity-70">
     feature が異なるため、共通プロパティ編集は無効。
    </div>
   {:else}
    <div class="space-y-2">
     {#each multiProps as item (item.p.name)}
      {@const cat = typeCategory(item.p.ty)}
      <div class={`rounded border p-2 ${item.mixed ? 'border-warning-500/60 bg-warning-500/5' : 'border-surface-200-800'}`}>
       <div class="mb-1 flex items-baseline justify-between gap-1">
        <label class="truncate font-semibold" for={`multi-prop-${item.p.name}`}>
         {item.p.label}
         {#if item.p.required}<span class="text-error-500">*</span>{/if}
        </label>
        <div class="flex items-center gap-1">
         {#if item.mixed}
          <span class="rounded bg-warning-500/25 px-1 text-[0.65rem] text-warning-900-100">mixed</span>
         {:else if item.presentCount !== selectedNodes.length}
          <span class="rounded bg-surface-300-700 px-1 text-[0.65rem]">partial</span>
         {/if}
         <span class="text-[0.65rem] opacity-60">{item.p.ty}</span>
         {#if item.presentCount === 0}
          <button
           type="button"
           class="rounded bg-primary-500 px-1.5 py-0.5 text-[0.65rem] text-white"
           onclick={() => setMultiPropToDefault(item.p)}
          >
           Set
          </button>
         {:else if !item.p.required}
          <button
           type="button"
           class="rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.65rem]"
           onclick={() => removeMultiProp(item.p.name)}
           title="デフォルトに戻す（キー削除）"
          >
           Unset
          </button>
         {/if}
        </div>
       </div>
       {#if item.p.description}
        <div class="mb-1 text-[0.65rem] opacity-60">{item.p.description}</div>
       {/if}
       {#if item.presentCount > 0}
        {#if cat === 'bool'}
         <label class="flex items-center gap-2">
          <input
           id={`multi-prop-${item.p.name}`}
           type="checkbox"
           checked={!item.mixed && asBool(item.value)}
           onchange={(e) => updateMultiProp(item.p.name, (e.target as HTMLInputElement).checked)}
          />
          <span>{item.mixed ? 'mixed' : asBool(item.value) ? 'true' : 'false'}</span>
         </label>
        {:else if cat === 'int'}
         <input
          id={`multi-prop-${item.p.name}`}
          type="number"
          step="1"
          class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
          value={item.mixed ? '' : asNumber(item.value)}
          placeholder={item.mixed ? 'mixed' : ''}
          oninput={(e) => updateMultiProp(item.p.name, parseInt((e.target as HTMLInputElement).value || '0', 10))}
         />
        {:else if cat === 'float'}
         <input
          id={`multi-prop-${item.p.name}`}
          type="number"
          step="any"
          class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
          value={item.mixed ? '' : asNumber(item.value)}
          placeholder={item.mixed ? 'mixed' : ''}
          oninput={(e) => updateMultiProp(item.p.name, parseFloat((e.target as HTMLInputElement).value || '0'))}
         />
        {:else if cat === 'string' && item.p.choices && item.p.choices.length > 0}
         <select
          id={`multi-prop-${item.p.name}`}
          class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
          value={item.mixed ? '' : asString(item.value)}
          onchange={(e) => updateMultiProp(item.p.name, (e.target as HTMLSelectElement).value)}
         >
          {#if item.mixed}<option value="">mixed</option>{/if}
          {#each item.p.choices as choice (choice)}
           <option value={choice}>{choice}</option>
          {/each}
         </select>
        {:else if cat === 'string'}
         <input
          id={`multi-prop-${item.p.name}`}
          type="text"
          class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
          value={item.mixed ? '' : asString(item.value)}
          placeholder={item.mixed ? 'mixed' : ''}
          oninput={(e) => updateMultiProp(item.p.name, (e.target as HTMLInputElement).value)}
         />
        {:else}
         <textarea
          id={`multi-prop-${item.p.name}`}
          class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-[0.7rem]"
          rows={item.mixed ? 3 : asJson(item.value).split('\n').length < 3 ? 3 : Math.min(10, asJson(item.value).split('\n').length)}
          value={item.mixed ? '' : asJson(item.value)}
          placeholder={item.mixed ? 'mixed' : ''}
          oninput={(e) => onMultiJsonChange(item.p.name, (e.target as HTMLTextAreaElement).value)}
         ></textarea>
        {/if}
       {:else}
        <div class="font-mono text-[0.7rem] opacity-60">
         default = {asJson(item.p.default)}
        </div>
       {/if}
      </div>
     {/each}
    </div>
   {/if}
  </div>
 {:else if !node}
  <div class="flex-1 overflow-y-auto p-3 text-xs opacity-60">ノードが選択されていません。</div>
 {:else if !spec}
  <div class="p-3 text-xs text-error-500">
   未知 feature: <code>{node.feature}</code>
  </div>
 {:else}
  <div class="flex-1 overflow-y-auto p-2 text-xs">
   <div class="mb-2 rounded bg-surface-100-900 p-2">
    <div class="mb-1 flex items-baseline justify-between gap-1">
     <span class="truncate font-semibold">{spec.title}</span>
     <span class="truncate text-[0.65rem] opacity-60">{spec.feature}</span>
    </div>
    <div class="font-mono text-[0.65rem] opacity-70">#{node.id}</div>
    {#if spec.description}
     <div class="mt-1 whitespace-pre-wrap text-[0.7rem] opacity-70">{spec.description}</div>
    {/if}
   </div>

   <div class="space-y-2">
    {#each propsView.known as item (item.p.name)}
     {@const cat = typeCategory(item.p.ty)}
     <div class="rounded border border-surface-200-800 p-2">
      <div class="mb-1 flex items-baseline justify-between gap-1">
       <label class="truncate font-semibold" for={`prop-${item.p.name}`}>
        {item.p.label}
        {#if item.p.required}<span class="text-error-500">*</span>{/if}
       </label>
       <div class="flex items-center gap-1">
        <span class="text-[0.65rem] opacity-60">{item.p.ty}</span>
        {#if !item.present}
         <button
          type="button"
          class="rounded bg-primary-500 px-1.5 py-0.5 text-[0.65rem] text-white"
          onclick={() => setPropToDefault(item.p)}
         >
          Set
         </button>
        {:else if !item.p.required}
         <button
          type="button"
          class="rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.65rem]"
          onclick={() => removeProp(item.p.name)}
          title="デフォルトに戻す（キー削除）"
         >
          Unset
         </button>
        {/if}
       </div>
      </div>
      {#if item.p.description}
       <div class="mb-1 text-[0.65rem] opacity-60">{item.p.description}</div>
      {/if}
      {#if item.present}
       {#if cat === 'bool'}
        <label class="flex items-center gap-2">
         <input
          id={`prop-${item.p.name}`}
          type="checkbox"
          checked={asBool(item.value)}
          onchange={(e) => updateProp(item.p.name, (e.target as HTMLInputElement).checked)}
         />
         <span>{asBool(item.value) ? 'true' : 'false'}</span>
        </label>
       {:else if cat === 'int'}
        <input
         id={`prop-${item.p.name}`}
         type="number"
         step="1"
         class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
         value={asNumber(item.value)}
         oninput={(e) => updateProp(item.p.name, parseInt((e.target as HTMLInputElement).value || '0', 10))}
        />
       {:else if cat === 'float'}
        <input
         id={`prop-${item.p.name}`}
         type="number"
         step="any"
         class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
         value={asNumber(item.value)}
         oninput={(e) => updateProp(item.p.name, parseFloat((e.target as HTMLInputElement).value || '0'))}
        />
      {:else if cat === 'string' && item.p.choices && item.p.choices.length > 0}
       <select
        id={`prop-${item.p.name}`}
        class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
        value={asString(item.value)}
        onchange={(e) => updateProp(item.p.name, (e.target as HTMLSelectElement).value)}
       >
        {#each item.p.choices as choice (choice)}
         <option value={choice}>{choice}</option>
        {/each}
       </select>
      {:else if cat === 'string' && isUnitStringProp(item.p)}
       <input
        id={`prop-${item.p.name}`}
        type="text"
        class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-[0.7rem]"
        value={asString(item.value)}
        oninput={(e) => {
         const v = (e.target as HTMLInputElement).value;
         updateProp(item.p.name, v);
         scheduleUnitParse(item.p.name, v);
        }}
       />
       {#if unitParseByProp[item.p.name]}
        <div class="mt-0.5 text-[0.65rem] text-error-600 dark:text-error-400">
         {unitParseByProp[item.p.name]}
        </div>
       {/if}
      {:else if cat === 'string'}
       <input
        id={`prop-${item.p.name}`}
        type="text"
        class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
        value={asString(item.value)}
        oninput={(e) => updateProp(item.p.name, (e.target as HTMLInputElement).value)}
       />
       {:else}
        <textarea
         id={`prop-${item.p.name}`}
         class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-[0.7rem]"
         rows={asJson(item.value).split('\n').length < 3 ? 3 : Math.min(10, asJson(item.value).split('\n').length)}
         value={asJson(item.value)}
         oninput={(e) => onJsonChange(item.p.name, (e.target as HTMLTextAreaElement).value)}
        ></textarea>
       {/if}
      {:else}
       <div class="font-mono text-[0.7rem] opacity-60">
        default = {asJson(item.p.default)}
       </div>
      {/if}
     </div>
    {/each}

    {#each propsView.unknown as item (item.name)}
     <div class="rounded border border-warning-500/50 p-2">
      <div class="mb-1 flex items-baseline justify-between gap-1">
       <span class="truncate font-semibold text-warning-700-300">{item.name}</span>
       <div class="flex items-center gap-1">
        <span class="rounded bg-warning-500/30 px-1 text-[0.65rem] text-warning-900-100">unknown</span>
        <button
         type="button"
         class="rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.65rem]"
         onclick={() => removeProp(item.name)}
        >
         Remove
        </button>
       </div>
      </div>
      <div class="font-mono text-[0.7rem] opacity-70">{asJson(item.value)}</div>
     </div>
    {/each}
   </div>
  </div>
 {/if}
 {#if visibleGroups.length > 0}
  <div class="border-t border-surface-200-800 p-2 text-xs">
   <div class="mb-2 flex items-center justify-between gap-2">
    <div class="font-semibold uppercase tracking-wider opacity-60">Groups</div>
    <div class="text-[0.65rem] opacity-60">{visibleGroups.length}</div>
   </div>
   <div class="space-y-2">
    {#each visibleGroups as group (group.id)}
     <div class="rounded border border-surface-200-800 p-2">
      <div class="mb-2 flex items-center justify-between gap-2">
       <div class="min-w-0">
        <div class="truncate font-mono text-[0.65rem] opacity-60">#{group.id}</div>
        <div class="truncate text-[0.65rem] opacity-60">{group.node_ids.length} nodes</div>
       </div>
       <div class="flex shrink-0 items-center gap-1">
        <button
         type="button"
         class="rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.65rem]"
         onclick={() => flowgraphStore.selectGroupNodes(group.id)}
        >
         Select
        </button>
        <button
         type="button"
         class="rounded border border-error-500/50 px-1.5 py-0.5 text-[0.65rem] text-error-700-300"
         onclick={() => flowgraphStore.removeGroup(group.id)}
        >
         Remove
        </button>
       </div>
      </div>
      <label class="mb-1 block text-[0.65rem] font-semibold opacity-70" for={`group-label-${group.id}`}>
       Label
      </label>
      <input
       id={`group-label-${group.id}`}
       type="text"
       class="mb-2 w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
       value={group.label ?? ''}
       placeholder={group.id}
       onchange={(e) => updateGroupLabel(group.id, (e.target as HTMLInputElement).value)}
      />
      <label class="mb-1 block text-[0.65rem] font-semibold opacity-70" for={`group-color-${group.id}`}>
       Color
      </label>
      <div class="flex items-center gap-2">
       <input
        id={`group-color-${group.id}`}
        type="color"
        class="h-7 w-10 rounded border border-surface-300-700 bg-surface-50-950"
        value={group.color ?? '#38bdf8'}
        onchange={(e) => updateGroupColor(group.id, (e.target as HTMLInputElement).value)}
       />
       <input
        aria-label={`Group color ${group.id}`}
        type="text"
        class="min-w-0 flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-[0.65rem]"
        value={group.color ?? ''}
        placeholder="#38bdf8"
        onchange={(e) => updateGroupColor(group.id, (e.target as HTMLInputElement).value)}
       />
      </div>
     </div>
    {/each}
   </div>
  </div>
 {/if}
</div>
