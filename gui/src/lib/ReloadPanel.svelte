<script lang="ts">
 // Control API `/reload` への UI。AI Persona / Modify Files の 2 系統をタブで切り替える。
 //
 // ## このパネルの位置付け（γ-8-f 再整理）
 //
 // **ここで行うのは「実行時だけの一時変更」または「外部編集後のメモリ反映」です。**
 // conf.toml への永続化はここでは行いません。永続化したい場合は:
 //   - Processor / AI Persona のプロパティ変更 → **Pipeline タブ** のノードプロパティエディタ
 //   - 辞書への行単位 add/remove           → **Live タブ** のクイック辞書追加 / 行 API
 //   - プロファイル丸ごとの編集            → **Setup タブ** の Profiles パネル
 //
 // 棲み分けを整理した後もこのパネルが生き残っているのは以下のニッチ用途があるため:
 //
 //   **AI Persona reload（実行時専用）**
 //     - 配信中に system_instructions_extra / heartbeat / decision_threshold を即試したい。
 //       `conf.toml` を汚さずロールバック前提で弄れる。
 //     - `custom_instructions` の **空文字送信** = クリアという少し奇妙なセマンティクスもここに閉じ込める。
 //
 //   **Modify Files reload（外部編集後のメモリ反映）**
 //     - ファイルを外部エディタ（γ-9 の委譲ボタン経由 or 手動）で書き換えた後、VAC を再起動せずに
 //       辞書/正規表現ファイルをディスクから再読み込み。
 //     - GUI の行 API 経由の変更は自動反映なので押す必要なし。ここを押すのは **ファイルを外部で弄ったとき** のみ。
 //
 // 重要セマンティクス（AiReloadRequest）:
 //   - フィールドを `null` にすると「変更しない」
 //   - `""` (空文字) にすると instructions を **クリア**
 //   - 値を入れると差し替え
 //   UI では各フィールドを「変更しない / 変更する」のトグルで切り替え、OFF なら送らない（`null`）、
 //   ON なら textarea の現在値を送る（空なら `""` が送信される＝クリアの意）
 //
 // Reload 成功時、サーバから `ControlEvent.reloaded` が飛んでくるので SnapshotView は自動追従する。

 import { onMount, onDestroy } from 'svelte';
 import { SvelteMap } from 'svelte/reactivity';
 import { api } from './api';
 import {
  ControlApiError,
  type AiPersonaSummary,
  type AiReloadReport,
  type AiReloadRequest,
  type ModifyReport,
  type ProcessorSummary,
  type StateSnapshot,
 } from './types';
 import { eventsStore } from './events.svelte';

 type Tab = 'ai' | 'modify';
 let tab: Tab = $state('ai');

 // ---- snapshot（選択肢を埋めるため）--------------------------------------
 let snapshot = $state<StateSnapshot | null>(null);
 let snapshotError = $state<string | null>(null);
 let unsubscribe: (() => void) | null = null;

 async function refreshSnapshot(): Promise<void> {
  try {
   snapshot = await api.snapshot();
   snapshotError = null;
  } catch (e) {
   snapshotError = e instanceof ControlApiError
    ? `${e.status} ${e.statusText}`
    : e instanceof Error
     ? e.message
     : String(e);
  }
 }

 onMount(() => {
  void refreshSnapshot();
  unsubscribe = eventsStore.subscribe((ev) => {
   if (ev.event.kind === 'reloaded') void refreshSnapshot();
  });
 });
 onDestroy(() => {
  if (unsubscribe) unsubscribe();
 });

 const aiPersonas: readonly AiPersonaSummary[] = $derived(snapshot?.ai_personas ?? []);
 const modifyProcessors: readonly ProcessorSummary[] = $derived.by(() => {
  if (!snapshot) return [];
  return snapshot.processors.filter((p) => p.feature === 'modify');
 });

 // ---- AI Persona form ----------------------------------------------------
 let aiTargetId: string | null = $state(null);

 // 各フィールドは "変更する?" のトグルを持つ。OFF = null を送る。
 let custEnabled = $state(false);
 let custText = $state('');
 let sysEnabled = $state(false);
 let sysText = $state('');
 /** 'skip' = null / 'on' = true / 'off' = false */
 let hbMode: 'skip' | 'on' | 'off' = $state('skip');
 let thEnabled = $state(false);
 let thValue = $state(0.5);

 let aiBusy = $state(false);
 let aiError: string | null = $state(null);
 let aiResult: AiReloadReport | null = $state(null);
 let aiResultTarget: string | null = $state(null);

 function resetAiForm(): void {
  custEnabled = false;
  custText = '';
  sysEnabled = false;
  sysText = '';
  hbMode = 'skip';
  thEnabled = false;
  thValue = 0.5;
 }

 async function submitAi(): Promise<void> {
  if (aiBusy) return;
  aiBusy = true;
  aiError = null;
  aiResult = null;
  const changes: AiReloadRequest = {
   custom_instructions: custEnabled ? custText : null,
   system_instructions_extra: sysEnabled ? sysText : null,
   heartbeat_enabled: hbMode === 'skip' ? null : hbMode === 'on',
   decision_threshold: thEnabled ? thValue : null,
  };
  try {
   const res = await api.reloadAiPersona(aiTargetId, changes);
   aiResult = res.ai_report ?? null;
   aiResultTarget = aiTargetId ?? '(all / default)';
  } catch (e) {
   aiError = e instanceof ControlApiError
    ? `${e.status} ${e.statusText}: ${JSON.stringify(e.body)}`
    : e instanceof Error
     ? e.message
     : String(e);
  } finally {
   aiBusy = false;
  }
 }

// ---- Modify Files form --------------------------------------------------
let modTargetId: string | null = $state(null);
let modBusy = $state(false);
let modError: string | null = $state(null);
let modResult: ModifyReport[] | null = $state(null);

// ---- External editor delegation (γ-9) -----------------------------------
// Modify processor の writable ファイルを OS 既定の関連付けアプリで開く。
// 対象は `modTargetId` の processor に紐付く writable_*_file。id 未指定（全 modify）のときは
// ボタンを無効化（どのファイルを開けばいいか決まらないので）。
let openBusy = $state(false);
let openError = $state<string | null>(null);
let openResult = $state<string | null>(null);

const modifyProcessorById = $derived.by(() => {
 const map = new SvelteMap<string, ProcessorSummary>();
 for (const p of modifyProcessors) {
  if (p.id) map.set(p.id, p);
 }
 return map;
});

const canOpenDictionary = $derived.by(() => {
 if (!modTargetId) return false;
 const p = modifyProcessorById.get(modTargetId);
 return !!p?.writable_dictionary_file;
});
const canOpenRegex = $derived.by(() => {
 if (!modTargetId) return false;
 const p = modifyProcessorById.get(modTargetId);
 return !!p?.writable_regex_file;
});

async function openExternal(kind: 'dictionary' | 'regex'): Promise<void> {
 if (!modTargetId || openBusy) return;
 openBusy = true;
 openError = null;
 openResult = null;
 try {
  const res = await api.openModifyFileExternal(modTargetId, kind);
  openResult = `${kind}: ${res.file} を起動しました`;
 } catch (e) {
  openError = e instanceof ControlApiError
   ? `${e.status} ${e.statusText}: ${JSON.stringify(e.body)}`
   : e instanceof Error
    ? e.message
    : String(e);
 } finally {
  openBusy = false;
 }
}

 async function submitModify(): Promise<void> {
  if (modBusy) return;
  modBusy = true;
  modError = null;
  modResult = null;
  try {
   const res = await api.reloadModifyFiles(modTargetId);
   modResult = res.modify_reports ?? [];
  } catch (e) {
   modError = e instanceof ControlApiError
    ? `${e.status} ${e.statusText}: ${JSON.stringify(e.body)}`
    : e instanceof Error
     ? e.message
     : String(e);
  } finally {
   modBusy = false;
  }
 }

 // "変更予定あり" の pill 表示用
 const aiHasChanges = $derived(custEnabled || sysEnabled || hbMode !== 'skip' || thEnabled);
</script>

<section class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
 <header class="mb-3 flex items-center justify-between">
  <h3 class="text-sm font-semibold">
   Reload <span class="opacity-60">(実行時のみ / 外部編集反映用)</span>
  </h3>
  {#if snapshotError}
   <span class="rounded bg-error-200-800 px-2 py-0.5 text-[10px] text-error-900-100">
    snapshot: {snapshotError}
   </span>
  {/if}
 </header>

 <!-- セマンティクスのヒント（γ-8-f）: 他タブへの誘導を兼ねる -->
 <p class="mb-3 rounded border border-surface-300-700 bg-surface-50-950 p-2 text-[11px] leading-relaxed opacity-80">
  ここは <strong>実行時だけの一時変更</strong> と
  <strong>外部編集後のメモリ反映</strong> のためのパネルです。<code>conf.toml</code> への永続保存は行いません。
  <br />
  <span class="opacity-70">
   永続化したい場合 → Processor/Persona は <strong>Pipeline タブ</strong>、辞書の行単位は
   <strong>Live タブ</strong>、プロファイル丸ごとは <strong>Profiles パネル</strong> から。
  </span>
 </p>

 <!-- タブ -->
 <div class="mb-3 flex gap-1 border-b border-surface-300-700">
  <button
   type="button"
   class="rounded-t px-3 py-1 text-xs font-semibold {tab === 'ai'
    ? 'bg-surface-50-950 text-primary-500'
    : 'opacity-60 hover:opacity-90'}"
   onclick={() => (tab = 'ai')}
  >
   AI Persona
  </button>
  <button
   type="button"
   class="rounded-t px-3 py-1 text-xs font-semibold {tab === 'modify'
    ? 'bg-surface-50-950 text-primary-500'
    : 'opacity-60 hover:opacity-90'}"
   onclick={() => (tab = 'modify')}
  >
   Modify Files
  </button>
 </div>

{#if tab === 'ai'}
 <div class="space-y-3 text-xs">
  <div class="rounded bg-warning-200-800/40 px-2 py-1 text-[11px] leading-relaxed">
   <strong>実行時のみ</strong>の変更。VAC 再起動や次のプロファイル読み込みで元に戻ります。<br />
   <span class="opacity-80">永続化したい場合は <strong>Pipeline タブ</strong>で該当ペルソナノードのプロパティを編集してください。</span>
  </div>
  <!-- 対象 -->
  <div class="flex items-center gap-2">
   <label class="font-semibold opacity-80" for="ai-target">対象</label>
    <select
     id="ai-target"
     class="flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono"
     bind:value={aiTargetId}
    >
     <option value={null}>（id 未指定 / サーバ既定）</option>
     {#each aiPersonas as p (p.index)}
      <option value={p.id}>#{p.index} · {p.id ?? '(no id)'}</option>
     {/each}
    </select>
   </div>

   <!-- custom_instructions -->
   <fieldset class="rounded border border-surface-200-800 p-2">
    <legend class="px-1 font-semibold opacity-80">
     <label class="flex items-center gap-1">
      <input type="checkbox" bind:checked={custEnabled} />
      custom_instructions を変更する
     </label>
    </legend>
    <textarea
     class="w-full rounded border border-surface-200-800 bg-surface-50-950 p-1 font-mono disabled:opacity-40"
     rows="4"
     placeholder="（空のまま送信すると instructions をクリアします）"
     bind:value={custText}
     disabled={!custEnabled}
    ></textarea>
   </fieldset>

   <!-- system_instructions_extra -->
   <fieldset class="rounded border border-surface-200-800 p-2">
    <legend class="px-1 font-semibold opacity-80">
     <label class="flex items-center gap-1">
      <input type="checkbox" bind:checked={sysEnabled} />
      system_instructions_extra を変更する
     </label>
    </legend>
    <textarea
     class="w-full rounded border border-surface-200-800 bg-surface-50-950 p-1 font-mono disabled:opacity-40"
     rows="3"
     placeholder="（空のまま送信するとクリアします）"
     bind:value={sysText}
     disabled={!sysEnabled}
    ></textarea>
   </fieldset>

   <!-- heartbeat_enabled -->
   <div class="flex items-center gap-3 rounded border border-surface-200-800 p-2">
    <span class="font-semibold opacity-80">heartbeat</span>
    <label class="flex items-center gap-1">
     <input type="radio" bind:group={hbMode} value="skip" /> 変更しない
    </label>
    <label class="flex items-center gap-1">
     <input type="radio" bind:group={hbMode} value="on" /> ON
    </label>
    <label class="flex items-center gap-1">
     <input type="radio" bind:group={hbMode} value="off" /> OFF
    </label>
   </div>

   <!-- decision_threshold -->
   <div class="flex items-center gap-3 rounded border border-surface-200-800 p-2">
    <label class="flex items-center gap-1 font-semibold opacity-80">
     <input type="checkbox" bind:checked={thEnabled} />
     decision_threshold を変更する
    </label>
    <input
     type="number"
     step="0.01"
     min="0"
     max="1"
     class="w-24 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-0.5 font-mono disabled:opacity-40"
     bind:value={thValue}
     disabled={!thEnabled}
    />
    <span class="opacity-60">0.0 〜 1.0</span>
   </div>

   <!-- 送信 -->
   <div class="flex items-center gap-2">
    <button
     type="button"
     class="rounded bg-primary-500 px-3 py-1 font-semibold text-primary-950 hover:bg-primary-400 disabled:opacity-50"
     onclick={() => void submitAi()}
     disabled={aiBusy || !aiHasChanges}
    >
     {aiBusy ? '送信中…' : '送信'}
    </button>
    <button
     type="button"
     class="rounded border border-surface-300-700 bg-surface-50-950 px-3 py-1 hover:bg-surface-200-800"
     onclick={resetAiForm}
     disabled={aiBusy}
    >
     フォームクリア
    </button>
    {#if !aiHasChanges}
     <span class="opacity-60">送信する変更がありません</span>
    {/if}
   </div>

   <!-- エラー / 結果 -->
   {#if aiError}
    <div class="rounded bg-error-200-800 px-2 py-1 text-xs text-error-900-100">
     ERROR: {aiError}
    </div>
   {:else if aiResult}
    {@const r = aiResult}
    <div class="rounded bg-success-200-800/60 px-2 py-2 text-xs">
     <div class="mb-1 font-semibold">
      reloaded · target={aiResultTarget}
     </div>
     <ul class="list-inside list-disc space-y-0.5 font-mono">
      <li>custom_instructions_changed: {r.custom_instructions_changed}</li>
      <li>system_instructions_extra_changed: {r.system_instructions_extra_changed}</li>
      <li>heartbeat_enabled_changed: {r.heartbeat_enabled_changed}</li>
      <li>decision_threshold_changed: {r.decision_threshold_changed}</li>
      <li>rebuilt_request_template: {r.rebuilt_request_template}</li>
      <li>rebuilt_decision_spec: {r.rebuilt_decision_spec}</li>
     </ul>
     {#if r.warnings.length > 0}
      <div class="mt-1 rounded bg-warning-200-800 p-1">
       <div class="font-semibold">warnings:</div>
       <ul class="list-inside list-disc">
        {#each r.warnings as w (w)}
         <li>{w}</li>
        {/each}
       </ul>
      </div>
     {/if}
    </div>
   {/if}
  </div>
 {:else}
  <!-- Modify Files タブ -->
  <div class="space-y-3 text-xs">
   <div class="flex items-center gap-2">
    <label class="font-semibold opacity-80" for="modify-target">対象</label>
    <select
     id="modify-target"
     class="flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono"
     bind:value={modTargetId}
    >
     <option value={null}>すべての modify processor</option>
     {#each modifyProcessors as p (p.index)}
      <option value={p.id}>#{p.index} · {p.id ?? '(no id)'}</option>
     {/each}
    </select>
   </div>

  <div class="rounded bg-warning-200-800/40 px-2 py-1 text-[11px] leading-relaxed">
   <strong>外部編集後のメモリ反映</strong>用。GUI 内の行単位 add/remove（Live タブ / 行 API）は自動反映されるので、押す必要はありません。<br />
   <span class="opacity-80">ファイルをテキストエディタ等で書き換えた後にここを押すと、VAC 再起動なしで反映されます。</span>
  </div>
  <p class="opacity-70">
   指定した modify processor の辞書・正規表現ファイルを再読み込みします。
  </p>

  <div class="flex flex-wrap items-center gap-2">
   <button
    type="button"
    class="rounded bg-primary-500 px-3 py-1 font-semibold text-primary-950 hover:bg-primary-400 disabled:opacity-50"
    onclick={() => void submitModify()}
    disabled={modBusy}
   >
    {modBusy ? '送信中…' : '再読み込み'}
   </button>
   <span class="opacity-50">|</span>
   <button
    type="button"
    class="rounded border border-surface-300-700 bg-surface-50-950 px-3 py-1 hover:bg-surface-200-800 disabled:opacity-40"
    onclick={() => void openExternal('dictionary')}
    disabled={openBusy || !canOpenDictionary}
    title={canOpenDictionary
     ? '辞書 writable ファイルを OS 既定のエディタで開きます'
     : '対象 id を選択し、writable_dictionary_file が設定された modify processor である必要があります'}
   >
    外部エディタで辞書を開く
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 bg-surface-50-950 px-3 py-1 hover:bg-surface-200-800 disabled:opacity-40"
    onclick={() => void openExternal('regex')}
    disabled={openBusy || !canOpenRegex}
    title={canOpenRegex
     ? '正規表現 writable ファイルを OS 既定のエディタで開きます'
     : '対象 id を選択し、writable_regex_file が設定された modify processor である必要があります'}
   >
    外部エディタで正規表現を開く
   </button>
  </div>
  {#if openError}
   <div class="rounded bg-error-200-800 px-2 py-1 text-xs text-error-900-100">
    外部エディタ起動エラー: {openError}
   </div>
  {:else if openResult}
   <div class="rounded bg-success-200-800/60 px-2 py-1 text-xs">
    {openResult}
    <span class="opacity-70"> — 編集後、左の「再読み込み」でメモリ反映してください。</span>
   </div>
  {/if}

   {#if modError}
    <div class="rounded bg-error-200-800 px-2 py-1 text-xs text-error-900-100">
     ERROR: {modError}
    </div>
   {:else if modResult}
    {#if modResult.length === 0}
     <div class="rounded bg-surface-200-800 px-2 py-1 opacity-80">
      該当 processor なし（selected_id にマッチする modify processor がありませんでした）
     </div>
    {:else}
     <table class="w-full border-collapse rounded bg-success-200-800/60 text-xs">
      <thead>
       <tr class="border-b border-success-500/40 text-left">
        <th class="px-2 py-1">#</th>
        <th class="px-2 py-1">id</th>
        <th class="px-2 py-1">feature</th>
        <th class="px-2 py-1 text-right">dict</th>
        <th class="px-2 py-1 text-right">regex</th>
       </tr>
      </thead>
      <tbody>
       {#each modResult as r (r.processor_index)}
        <tr class="border-b border-success-500/20 last:border-b-0">
         <td class="px-2 py-1 font-mono opacity-60">{r.processor_index}</td>
         <td class="px-2 py-1 font-mono">{r.processor_id ?? '—'}</td>
         <td class="px-2 py-1 font-mono">{r.feature}</td>
         <td class="px-2 py-1 text-right font-mono">{r.dictionary_entries}</td>
         <td class="px-2 py-1 text-right font-mono">{r.regex_entries}</td>
        </tr>
       {/each}
      </tbody>
     </table>
    {/if}
   {/if}
  </div>
 {/if}
</section>
