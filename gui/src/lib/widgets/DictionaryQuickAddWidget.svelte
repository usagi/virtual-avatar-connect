<script lang="ts">
 /**
  * Phase VI-γ-8c: Live タブの「クイック辞書追加」ウィジェット。
  *
  * - `api.snapshot()` の `processors[]` から **`feature === "modify"` かつ
  *   `writable_dictionary_file` が設定されている processor** を抽出して選択肢にする。
  * - to / from の 2 入力 + 「追加」ボタン。Ctrl+Enter でも送信。
  * - 追加結果は直近 5 件を履歴として表示し、各行の「取消」で `api.removeDictionaryEntry` を呼ぶ。
  * - 対象 processor の選択は `localStorage` に永続化。
  * - `ControlEvent.reloaded` を購読して snapshot を再取得（他クライアントや外部編集で変化したとき）。
  *
  * 実装メモ:
  *   - writable_regex は扱わない。regex は pattern 検証や UX が辞書と違うので γ-8-c の別ウィジェット or
  *     パイプラインエディタ側で扱う（γ-8-e 予定）。
  *   - ここでは "id なしの modify processor" は選べない（API が id を要求するため）。
  *     そのような processor がある場合は UI にヒントを出す。
  */

 import { onMount, onDestroy } from 'svelte';
 import { api } from '../api';
 import {
  ControlApiError,
  type DictionaryEntryRequest,
  type DictionaryEntryResponse,
  type ProcessorSummary,
  type StateSnapshot,
 } from '../types';
 import { toastStore } from '../toasts.svelte';
 import { eventsStore } from '../events.svelte';

 const LS_TARGET = 'vac.dictionary_quick.target_id';
 const HISTORY_MAX = 5;

 type HistoryItem = {
  seq: number;
  target_id: string;
  to: string;
  from: string;
  total_entries: number;
  already_present: boolean;
  removed: boolean;
  at: string;
 };

 let loading = $state(true);
 let loadError = $state<string | null>(null);
 let processors = $state<ProcessorSummary[]>([]);

 let targetId = $state<string>(localStorage.getItem(LS_TARGET) ?? '');
 let inputTo = $state('');
 let inputFrom = $state('');
 let submitting = $state(false);
 let history = $state<HistoryItem[]>([]);
 let busyHistorySeq = $state<number | null>(null);
 let nextSeq = 1;

 let unsubEvents: (() => void) | null = null;

 const writableProcessors = $derived(
  processors.filter(
   (p): p is ProcessorSummary & { id: string; writable_dictionary_file: string } =>
    p.feature === 'modify' && p.id !== null && !!p.writable_dictionary_file,
  ),
 );

 const hasWritableTargets = $derived(writableProcessors.length > 0);
 const hasModifyWithoutId = $derived(
  processors.some(
   (p) => p.feature === 'modify' && (p.id === null || !p.writable_dictionary_file),
  ),
 );

 const selectedProcessor = $derived(
  writableProcessors.find((p) => p.id === targetId) ?? null,
 );

 // writable_dictionary_file がある processor が現れたら、ターゲット未選択時は最初のものを既定にする。
 $effect(() => {
  if (!targetId && writableProcessors.length > 0) {
   targetId = writableProcessors[0].id;
  }
 });

 // ターゲット ID の永続化
 $effect(() => {
  if (targetId) {
   localStorage.setItem(LS_TARGET, targetId);
  }
 });

 function extractMessage(e: unknown): string {
  if (e instanceof ControlApiError) {
   const body = e.body as { reason?: string; error?: string } | null;
   return body?.reason ?? body?.error ?? `${e.status} ${e.statusText}`;
  }
  if (e instanceof Error) return e.message;
  return String(e);
 }

 async function refreshSnapshot(): Promise<void> {
  try {
   const snap: StateSnapshot = await api.snapshot();
   processors = snap.processors;
   loadError = null;
  } catch (e) {
   loadError = extractMessage(e);
  } finally {
   loading = false;
  }
 }

 const hasWhitespace = (s: string): boolean => /\s/u.test(s);

 function pushHistory(item: HistoryItem): void {
  history = [item, ...history].slice(0, HISTORY_MAX);
 }

 async function submit(): Promise<void> {
  if (submitting) return;
  const to = inputTo.trim();
  const from = inputFrom.trim();
  if (!targetId) {
   toastStore.warn('対象 Modify が選択されていません');
   return;
  }
  if (!to || !from) {
   toastStore.warn('to / from を両方入力してください');
   return;
  }
  if (hasWhitespace(to) || hasWhitespace(from)) {
   toastStore.warn('to / from に空白文字は使えません', '現行フォーマットは「to<space>from」のため');
   return;
  }
  submitting = true;
  const req: DictionaryEntryRequest = { to, from };
  try {
   const res: DictionaryEntryResponse = await api.addDictionaryEntry(targetId, req);
   if (res.already_present) {
    toastStore.info(
     '既に登録されていました',
     `${to} <- ${from}（total=${res.total_entries}）`,
    );
   } else {
    toastStore.success(
     '辞書に追加しました',
     `${to} <- ${from}（total=${res.total_entries}）`,
    );
   }
   pushHistory({
    seq: nextSeq++,
    target_id: targetId,
    to,
    from,
    total_entries: res.total_entries,
    already_present: res.already_present,
    removed: false,
    at: new Date().toISOString(),
   });
   inputTo = '';
   inputFrom = '';
  } catch (e) {
   toastStore.error('追加に失敗しました', extractMessage(e));
  } finally {
   submitting = false;
  }
 }

 async function undo(item: HistoryItem): Promise<void> {
  if (busyHistorySeq !== null) return;
  busyHistorySeq = item.seq;
  try {
   const res = await api.removeDictionaryEntry(item.target_id, {
    to: item.to,
    from: item.from,
   });
   if (res.removed_lines > 0) {
    toastStore.success(
     '辞書から削除しました',
     `${item.to} <- ${item.from}（${res.removed_lines} 行、total=${res.total_entries}）`,
    );
    history = history.map((h) =>
     h.seq === item.seq ? { ...h, removed: true } : h,
    );
   } else {
    toastStore.info(
     '削除対象が見当たりませんでした',
     '既に別のルートで削除済みか、他ファイル由来のエントリです',
    );
    history = history.map((h) =>
     h.seq === item.seq ? { ...h, removed: true } : h,
    );
   }
  } catch (e) {
   toastStore.error('取消に失敗しました', extractMessage(e));
  } finally {
   busyHistorySeq = null;
  }
 }

 function onTextareaKeydown(ev: KeyboardEvent): void {
  // Ctrl+Enter / Cmd+Enter で送信
  if ((ev.ctrlKey || ev.metaKey) && ev.key === 'Enter') {
   ev.preventDefault();
   void submit();
  }
 }

 onMount(() => {
  void refreshSnapshot();
  unsubEvents = eventsStore.subscribe((ev) => {
   // Reloaded / Restarting で再取得
   if (ev.event.kind === 'reloaded' || ev.event.kind === 'restarting') {
    void refreshSnapshot();
   }
  });
 });

 onDestroy(() => {
  if (unsubEvents) unsubEvents();
 });
</script>

<section class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
 <header class="mb-3 flex items-center justify-between gap-2">
  <div>
   <h3 class="text-sm font-semibold">クイック辞書追加</h3>
   <p class="text-xs opacity-70">
    配信中の誤読をその場で辞書へ。<code>writable_dictionary_file</code> 設定済みの Modify プロセッサが対象。
   </p>
  </div>
  <button
   type="button"
   class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-0.5 text-xs hover:bg-surface-200-800"
   onclick={() => void refreshSnapshot()}
   disabled={loading}
   title="snapshot を再取得"
  >
   ↻
  </button>
 </header>

 {#if loading}
  <div class="text-xs opacity-70">読み込み中…</div>
 {:else if loadError}
  <div class="rounded bg-error-200-800 px-2 py-1 text-xs text-error-900-100">
   ERROR: {loadError}
  </div>
 {:else if !hasWritableTargets}
  <div class="space-y-1 rounded bg-warning-200-800/60 px-2 py-2 text-xs">
   <div class="font-semibold">
    このウィジェットで操作できる Modify プロセッサがありません。
   </div>
   <ul class="list-inside list-disc opacity-80">
    <li>
      <code>[[processors]] feature = "modify"</code> の entry に
      <code>id = "..."</code> と <code>writable_dictionary_file = "..."</code> を追加してください。
    </li>
    <li>
     <code>writable_dictionary_file</code> は
     <code>dictionary_files</code> のいずれかに含まれている必要があります（辞書ファイル本体）。
    </li>
    <li>変更後は VAC を再起動するか Setup タブから再起動してください。</li>
   </ul>
  </div>
 {:else}
  <div class="space-y-3">
   <!-- 対象 select -->
   <div class="flex items-center gap-2 text-xs">
    <label class="font-semibold opacity-80" for="dq-target">対象</label>
    <select
     id="dq-target"
     class="flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono"
     bind:value={targetId}
    >
     {#each writableProcessors as p (p.index)}
      <option value={p.id}>
       #{p.index} · {p.id} ({p.writable_dictionary_file})
      </option>
     {/each}
    </select>
   </div>

   {#if hasModifyWithoutId}
    <p class="rounded bg-surface-200-800 px-2 py-1 text-[11px] opacity-80">
     <span class="font-semibold">ヒント:</span>
     他にも <code>modify</code> プロセッサがありますが、
     <code>id</code> が無い / <code>writable_dictionary_file</code> が未設定のため対象外です。
    </p>
   {/if}

   <!-- 入力欄 -->
   <div class="grid grid-cols-1 gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
    <label class="text-xs">
     <div class="mb-0.5 font-semibold opacity-80">to（読み / 置換後）</div>
     <input
      type="text"
      class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono"
      placeholder="ドクターウサギ"
      bind:value={inputTo}
      onkeydown={onTextareaKeydown}
      disabled={submitting}
     />
    </label>
    <label class="text-xs">
     <div class="mb-0.5 font-semibold opacity-80">from（元テキスト）</div>
     <input
      type="text"
      class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono"
      placeholder="Dr.USAGI"
      bind:value={inputFrom}
      onkeydown={onTextareaKeydown}
      disabled={submitting}
     />
    </label>
    <div class="flex items-end">
     <button
      type="button"
      class="h-[calc(1.75rem+2px)] rounded bg-primary-500 px-3 text-xs font-semibold text-primary-950 hover:bg-primary-400 disabled:opacity-50"
      onclick={() => void submit()}
      disabled={submitting || !targetId}
      title="Ctrl+Enter でも送信"
     >
      {submitting ? '送信中…' : '追加'}
     </button>
    </div>
   </div>

   <p class="text-[11px] opacity-60">
    <code>Ctrl+Enter</code> で送信。<code>to</code> / <code>from</code> に空白文字は使えません（現行フォーマット都合）。
   </p>

   <!-- 履歴 -->
   {#if history.length > 0}
    <div class="mt-2 rounded border border-surface-200-800 p-2">
     <div class="mb-1 text-[11px] font-semibold opacity-80">
      最近の操作（直近 {HISTORY_MAX} 件 / セッション内）
     </div>
     <ul class="space-y-1 text-[11px]">
      {#each history as item (item.seq)}
       <li
        class="flex items-center justify-between gap-2 rounded bg-surface-50-950 px-2 py-1 {item.removed
         ? 'opacity-50'
         : ''}"
       >
        <div class="min-w-0 flex-1">
         <span class="font-mono">{item.to}</span>
         <span class="opacity-60">←</span>
         <span class="font-mono">{item.from}</span>
         <span class="ml-2 rounded bg-surface-200-800 px-1 text-[10px] opacity-80">
          {item.target_id}
         </span>
         {#if item.already_present}
          <span class="ml-1 rounded bg-warning-200-800 px-1 text-[10px]">
           既存
          </span>
         {/if}
         {#if item.removed}
          <span class="ml-1 rounded bg-surface-300-700 px-1 text-[10px]">
           取消済
          </span>
         {/if}
        </div>
        <button
         type="button"
         class="rounded border border-surface-300-700 bg-surface-100-900 px-2 py-0.5 text-[10px] hover:bg-surface-200-800 disabled:opacity-50"
         onclick={() => void undo(item)}
         disabled={item.removed || busyHistorySeq === item.seq}
        >
         {busyHistorySeq === item.seq ? '…' : item.removed ? '—' : '取消'}
        </button>
       </li>
      {/each}
     </ul>
    </div>
   {/if}

   {#if selectedProcessor}
    <div class="text-[10px] opacity-50">
     書き込み先ファイル: <code>{selectedProcessor.writable_dictionary_file}</code>
    </div>
   {/if}
  </div>
 {/if}
</section>
