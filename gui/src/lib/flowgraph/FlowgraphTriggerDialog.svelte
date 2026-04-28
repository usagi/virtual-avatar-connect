<script lang="ts">
 /**
  * Phase φ-6: Flowgraph Editor 上の単体ノードを Control API から 1-shot 発火するためのダイアログ。
  *
  * - `NodeDescriptor::control_triggerable()` に opt-in しているノード（現状は
  *   `flowgraph.glossary.learn` / `.forget` のみ）に対してだけ、`FlowgraphNodeCard.svelte`
  *   から ▶ ボタン経由で開かれる想定。
  * - ユーザは各入力ポートに対して「このトリガで上書きしたい値」を手入力できる。
  *   値は JSON としてパースされ、失敗時は string として送る。
  * - `exec_port` は exec 入力が 2 本以上あるノードでだけ select を出す。
  * - 送信は `api.triggerFlowgraphNode(fqNodeId, body, instanceId)` をそのまま使う。
  *
  * 安全性は server 側（`control_triggerable` opt-in と JSON→SocketValue の厳格 coercion）に
  * 依存しており、このダイアログ自体は単なる UI convenience。型不正を叩いても 400/422 で弾かれる。
  */

 import { api } from '../api';
 import { ControlApiError } from '../types';
 import type { FlowgraphNodeSpec, FlowgraphPortSpec, TriggerNodeRequest } from '../types';
 import { toastStore } from '../toasts.svelte';

 type Props = {
  open: boolean;
  /** トリガ先の fq node_id（例: `chat-echo/main::learn`）。 */
  fqNodeId: string;
  spec: FlowgraphNodeSpec;
  /** V2 は常に `"default"`。 */
  instanceId?: string;
  onClose?: () => void;
 };

 let {
  open = $bindable(false),
  fqNodeId,
  spec,
  instanceId = 'default',
  onClose,
 }: Props = $props();

 type InputDraft = {
  override: boolean;
  /** raw text box（空なら default 扱い / JSON パース対象）。 */
  text: string;
 };

 const execInputs: FlowgraphPortSpec[] = $derived.by(() => spec.inputs.filter((p) => p.is_exec));
 const dataInputs: FlowgraphPortSpec[] = $derived.by(() => spec.inputs.filter((p) => !p.is_exec));

 let selectedExec: string = $state('');
 let drafts: Record<string, InputDraft> = $state({});
 let submitting = $state(false);
 let formError = $state<string | null>(null);

 /**
  * spec / open の切り替わりで state を初期化する。exec ポートは先頭を既定選択、
  * data ポートは override=false（=送らない）で start。
  */
 $effect(() => {
  if (!open) return;
  selectedExec = execInputs[0]?.name ?? '';
  const next: Record<string, InputDraft> = {};
  for (const p of dataInputs) {
   next[p.name] = { override: false, text: stringifyDefault(p.default) };
  }
  drafts = next;
  formError = null;
 });

 function stringifyDefault(v: unknown): string {
  if (v === undefined || v === null) return '';
  if (typeof v === 'string') return v;
  try {
   return JSON.stringify(v);
  } catch {
   return String(v);
  }
 }

 /**
  * port.ty から draft.text をサーバ送信用 JSON 値に変換する。
  *
  * - `string` ポートは text をそのまま string として送る（JSON パースを先にするとクオート必須になるため）。
  *   ただし text が `"..."` で囲われていたら JSON としてアンクォートする（人間のタイプの揺れ吸収）。
  * - それ以外は JSON.parse を試み、失敗したら string として送る（サーバが 422 を返してくれる）。
  */
 function coerceDraft(port: FlowgraphPortSpec, text: string): unknown {
  if (port.ty === 'string') {
   const trimmed = text.trim();
   if (trimmed.startsWith('"') && trimmed.endsWith('"')) {
    try {
     return JSON.parse(trimmed);
    } catch {
     // fallthrough
    }
   }
   return text;
  }
  const trimmed = text.trim();
  if (trimmed === '') return null;
  try {
   return JSON.parse(trimmed);
  } catch {
   // サーバ 422 に任せる。
   return text;
  }
 }

 async function onSubmit(): Promise<void> {
  if (submitting) return;
  submitting = true;
  formError = null;
  try {
   const inputs: Record<string, unknown> = {};
   for (const p of dataInputs) {
    const d = drafts[p.name];
    if (!d || !d.override) continue;
    inputs[p.name] = coerceDraft(p, d.text);
   }

   const body: TriggerNodeRequest = {};
   if (execInputs.length > 1 && selectedExec) body.exec_port = selectedExec;
   if (Object.keys(inputs).length > 0) body.inputs = inputs;

   const resp = await api.triggerFlowgraphNode(fqNodeId, body, instanceId);
   toastStore.success(
    'Trigger 送信 OK',
    `node=${resp.node_id} exec=${resp.exec_port} overrides=${resp.overridden_inputs.length}`,
   );
   open = false;
   onClose?.();
  } catch (e) {
   if (e instanceof ControlApiError) {
    formError = `[${e.status} ${e.statusText}] ${e.message}`;
   } else if (e instanceof Error) {
    formError = e.message;
   } else {
    formError = String(e);
   }
  } finally {
   submitting = false;
  }
 }

 function onKeyDown(ev: KeyboardEvent): void {
  if (!open) return;
  if (ev.key === 'Escape' && !submitting) {
   open = false;
   onClose?.();
  } else if (ev.key === 'Enter' && ev.ctrlKey && !submitting) {
   void onSubmit();
  }
 }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if open}
 <div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
  role="dialog"
  aria-modal="true"
  aria-labelledby="flowgraph-trigger-title"
 >
  <div class="w-full max-w-2xl rounded-lg border border-primary-500/50 bg-surface-50-950 shadow-xl">
   <header class="flex items-center justify-between border-b border-surface-200-800 px-5 py-3">
    <h2 id="flowgraph-trigger-title" class="text-sm font-semibold text-primary-700-300">
     ▶ Trigger Node
    </h2>
    <button
     type="button"
     class="text-lg leading-none opacity-70 hover:opacity-100"
     onclick={() => {
      open = false;
      onClose?.();
     }}
     disabled={submitting}
     aria-label="閉じる">×</button>
   </header>

   <div class="max-h-[70vh] space-y-3 overflow-y-auto px-5 py-4 text-xs">
    <div class="rounded bg-surface-100-900 p-2">
     <div class="opacity-70">feature</div>
     <code class="font-mono text-[0.7rem]">{spec.feature}</code>
     <div class="mt-1 opacity-70">node_id</div>
     <code class="font-mono text-[0.7rem]">{fqNodeId}</code>
    </div>

    {#if execInputs.length > 1}
     <div>
      <div class="mb-1 font-semibold uppercase tracking-wider opacity-60">exec_port</div>
      <select
       bind:value={selectedExec}
       class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1"
       disabled={submitting}
      >
       {#each execInputs as p (p.name)}
        <option value={p.name}>{p.label} ({p.name})</option>
       {/each}
      </select>
     </div>
    {:else if execInputs.length === 1}
     <div class="opacity-70">
      exec_port: <code class="font-mono">{execInputs[0].name}</code>（唯一のため自動選択）
     </div>
    {/if}

    <div>
     <div class="mb-1 flex items-center justify-between">
      <div class="font-semibold uppercase tracking-wider opacity-60">data inputs (override)</div>
      <div class="opacity-60">
       未チェックのポートは flowgraph 側の上流/デフォルトが使われます
      </div>
     </div>
     {#if dataInputs.length === 0}
      <div class="opacity-60">このノードに data input はありません。</div>
     {:else}
      <div class="overflow-x-auto rounded border border-surface-300-700">
       <table class="w-full border-collapse text-left">
        <thead class="bg-surface-100-900 text-[0.65rem] uppercase opacity-80">
         <tr>
          <th class="w-10 px-2 py-1 text-center">送る</th>
          <th class="px-2 py-1">port</th>
          <th class="px-2 py-1">ty</th>
          <th class="px-2 py-1">value</th>
         </tr>
        </thead>
        <tbody>
         {#each dataInputs as p (p.name)}
          <tr class="border-t border-surface-200-800 align-top">
           <td class="px-2 py-1 text-center">
            <input
             type="checkbox"
             bind:checked={drafts[p.name].override}
             disabled={submitting}
             aria-label={`${p.name} を送信`}
            />
           </td>
           <td class="px-2 py-1">
            <div class="font-mono">{p.name}</div>
            <div class="text-[0.65rem] opacity-60">{p.label}</div>
           </td>
           <td class="px-2 py-1 font-mono text-[0.65rem]">
            {p.ty}{p.optional ? '?' : ''}
           </td>
           <td class="px-2 py-1">
            <textarea
             bind:value={drafts[p.name].text}
             class="min-h-[2rem] w-full resize-y rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-[0.7rem]"
             rows={p.ty === 'string' ? 1 : 2}
             placeholder={p.ty === 'string' ? 'text' : 'JSON literal'}
             disabled={submitting || !drafts[p.name].override}
            ></textarea>
            {#if p.description}
             <div class="mt-0.5 text-[0.6rem] opacity-60">{p.description}</div>
            {/if}
           </td>
          </tr>
         {/each}
        </tbody>
       </table>
      </div>
     {/if}
    </div>

    {#if formError}
     <div class="rounded border border-error-500/40 bg-error-500/10 p-2 text-error-900-100">
      {formError}
     </div>
    {/if}

    <div class="text-[0.65rem] opacity-60">
     ヒント: <kbd>Ctrl</kbd>+<kbd>Enter</kbd> で Trigger、<kbd>Esc</kbd> で閉じる。
     サーバ側で
     <code>NodeDescriptor::control_triggerable()</code>
     が <code>true</code> のノードのみ発火可能（それ以外は 403 で拒否されます）。
    </div>
   </div>

   <footer class="flex items-center justify-end gap-2 border-t border-surface-200-800 px-5 py-3">
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1"
     onclick={() => {
      open = false;
      onClose?.();
     }}
     disabled={submitting}
    >
     キャンセル
    </button>
    <button
     type="button"
     class="rounded bg-primary-500 px-3 py-1 text-white shadow-sm hover:bg-primary-600 disabled:opacity-50"
     onclick={() => void onSubmit()}
     disabled={submitting}
    >
     {submitting ? '送信中…' : '▶ Trigger'}
    </button>
   </footer>
  </div>
 </div>
{/if}
