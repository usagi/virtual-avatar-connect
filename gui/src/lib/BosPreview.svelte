<script lang="ts">
 /**
  * Phase VI-γ-2a: Live タブの字幕プレビュー / OBS 用ブラウザソース URL コピー。
  *
  * 機能:
  *   - `GET /api/v1/control/bos` の結果を取得してセレクターで選ぶ
  *   - `supports_channel_param` のエントリには `channel` 入力欄を出し、`?channel=xxx` に反映
  *   - 現在の絶対 URL を表示し、コピーボタン（`navigator.clipboard`、失敗したら execCommand フォールバック）
  *   - iframe に指定 URL を埋め込み（OBS での見た目をそのまま確認）
  *   - 「別タブで開く」ボタンで単独検証も容易
  *
  * 非目標（γ-2a）:
  *   - テーマ切替や装飾パラメータの UI（γ-δ で `bos.json` スキーマを導入して拡張予定）
  *   - VAC 側のイベントをリアルタイムに描画する独自字幕（既存 poll.js で十分）
  */
 import { untrack } from 'svelte';
 import { api } from './api';
 import { ControlApiError, type BosEntry, type BosResponse } from './types';
 import { toastStore } from './toasts.svelte';

 interface Props {
  /** 親 (LiveTab) が指定するデフォルトの channel 文字列。未指定なら 'ai'。 */
  defaultChannel?: string;
 }
 const { defaultChannel = 'ai' }: Props = $props();

 let loading = $state(true);
 let error = $state<string | null>(null);
 let resp = $state<BosResponse | null>(null);
 let selectedId = $state<string | null>(null);
 // 親の defaultChannel はマウント時の初期値のみ反映（変動は想定しない）。
 // untrack で囲むことで state_referenced_locally 警告を抑制しつつ意図を明示する。
 let channel = $state(untrack(() => defaultChannel));

 const origin = typeof window !== 'undefined' ? window.location.origin : '';

 async function load() {
  loading = true;
  error = null;
  try {
   resp = await api.bos();
   if (resp.entries.length > 0 && (selectedId == null || !resp.entries.some((e) => e.id === selectedId))) {
    // 字幕カテゴリを優先して初期選択、無ければ先頭
    const firstSub = resp.entries.find((e) => e.category === 'subtitles');
    selectedId = firstSub?.id ?? resp.entries[0].id;
   }
  } catch (e) {
   error =
    e instanceof ControlApiError
     ? `${e.status} ${e.statusText}`
     : e instanceof Error
      ? e.message
      : String(e);
  } finally {
   loading = false;
  }
 }

 $effect(() => {
  void load();
 });

 const selectedEntry: BosEntry | null = $derived(
  resp && selectedId ? resp.entries.find((e) => e.id === selectedId) ?? null : null,
 );

 /** channel パラメータを載せたときの相対 URL（supports_channel_param かつ channel 非空なら付与）。 */
 const composedUrl = $derived.by(() => {
  if (!selectedEntry) return '';
  const base = selectedEntry.url;
  if (!selectedEntry.supports_channel_param) return base;
  const ch = channel.trim();
  if (ch === '') return base;
  const sep = base.includes('?') ? '&' : '?';
  return `${base}${sep}channel=${encodeURIComponent(ch)}`;
 });

 const absoluteUrl = $derived(composedUrl ? `${origin}${composedUrl}` : '');

 // γ-2: 縦画面（portrait）では iframe プレビューは邪魔になりやすいので折りたたむ。
 // 既定は横画面（landscape）だと展開、縦画面だと折りたたみ。ユーザ操作が優先される。
 let previewManuallyToggled = $state(false);
 let previewOpenOverride = $state(false);
 let isPortrait = $state(false);
 const previewOpen = $derived(previewManuallyToggled ? previewOpenOverride : !isPortrait);

 $effect(() => {
  if (typeof window === 'undefined') return;
  const mq = window.matchMedia('(orientation: portrait)');
  const apply = () => (isPortrait = mq.matches);
  apply();
  mq.addEventListener('change', apply);
  return () => mq.removeEventListener('change', apply);
 });

 function togglePreview() {
  previewManuallyToggled = true;
  previewOpenOverride = !previewOpen;
 }

 async function copyUrl() {
  if (!absoluteUrl) return;
  try {
   if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(absoluteUrl);
    toastStore.success('URL をコピーしました', absoluteUrl);
    return;
   }
  } catch {
   // fallthrough
  }
  // フォールバック: 一時 textarea + execCommand
  try {
   const ta = document.createElement('textarea');
   ta.value = absoluteUrl;
   ta.style.position = 'fixed';
   ta.style.opacity = '0';
   document.body.appendChild(ta);
   ta.select();
   const ok = document.execCommand('copy');
   document.body.removeChild(ta);
   if (ok) {
    toastStore.success('URL をコピーしました', absoluteUrl);
   } else {
    toastStore.error('コピーに失敗しました', '手動でコピーしてください。');
   }
  } catch (e) {
   toastStore.error('コピーに失敗しました', e instanceof Error ? e.message : String(e));
  }
 }

 function openInNewTab() {
  if (absoluteUrl) window.open(absoluteUrl, '_blank', 'noopener');
 }

 function categoryLabel(c: BosEntry['category']): string {
  switch (c) {
   case 'subtitles':
    return '字幕';
   case 'bgm':
    return 'BGM';
   case 'effects':
    return 'エフェクト';
   case 'other':
    return 'その他';
  }
 }
</script>

<section class="rounded-lg border border-surface-200-800 bg-surface-100-900 p-4">
 <header class="mb-2 flex items-baseline justify-between gap-2">
  <div>
   <h2 class="text-sm font-semibold">字幕プレビュー (Broadcast Output Sources)</h2>
   <p class="text-xs opacity-60">
    OBS のブラウザソースとして使える URL。選択中のソースは下の iframe でそのまま表示されます。
   </p>
  </div>
  <button
   type="button"
   class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-200-800"
   onclick={() => void load()}
   disabled={loading}
  >
   再読込
  </button>
 </header>

 {#if loading}
  <p class="text-xs opacity-70">読み込み中…</p>
 {:else if error}
  <p class="rounded border border-error-500/40 bg-error-500/10 p-2 text-xs">
   BOS 一覧の取得に失敗しました: {error}
  </p>
 {:else if !resp || resp.entries.length === 0}
  <p class="rounded border border-warning-500/40 bg-warning-500/10 p-2 text-xs">
   BOS エントリが見つかりませんでした。
   {#if resp?.document_root}
    <br />
    走査対象: <code>{resp.document_root}</code>
   {/if}
   <br />
   <code>resources/browser-output</code> を <code>conf.browser_source.document_root</code> に指定しているか確認してください。
  </p>
 {:else}
  <div class="flex flex-wrap items-end gap-3">
   <label class="flex flex-col">
    <span class="text-xs opacity-70">ソース</span>
    <select
     class="min-w-[16rem] rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-sm"
     bind:value={selectedId}
    >
     {#each resp.entries as e (e.id)}
      <option value={e.id}>
       [{categoryLabel(e.category)}] {e.title}{e.user_defined ? ' *' : ''}
      </option>
     {/each}
    </select>
    <span class="text-[10px] opacity-50">* = ユーザー定義（同梱外）</span>
   </label>

   {#if selectedEntry?.supports_channel_param}
    <label class="flex flex-col">
     <span class="text-xs opacity-70">channel</span>
     <input
      type="text"
      class="w-32 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-sm"
      bind:value={channel}
      placeholder="ai"
     />
    </label>
   {/if}
  </div>

  <div class="mt-3 flex items-center gap-2">
   <input
    type="text"
    class="flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
    value={absoluteUrl}
    readonly
    aria-label="Browser source URL"
   />
   <button
    type="button"
    class="rounded bg-primary-500 px-3 py-1 text-xs font-semibold text-white hover:bg-primary-600"
    onclick={copyUrl}
    disabled={!absoluteUrl}
   >
    URL コピー
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-200-800"
    onclick={openInNewTab}
    disabled={!absoluteUrl}
   >
    別タブで開く
   </button>
  </div>

  {#if composedUrl}
   <div class="mt-3">
    <button
     type="button"
     class="flex w-full items-center justify-between rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs hover:bg-surface-200-800"
     onclick={togglePreview}
     aria-expanded={previewOpen}
    >
     <span>プレビュー ({previewOpen ? '閉じる' : '開く'})</span>
     <span class="opacity-60">{isPortrait ? '縦画面' : '横画面'}</span>
    </button>
    {#if previewOpen}
     <div class="mt-1 overflow-hidden rounded border border-surface-300-700 bg-black">
      <!-- iframe の src は絶対 URL でも相対 URL でも良い。相対にしておくと origin を切り替えても追従する -->
      <iframe
       title={selectedEntry?.title ?? 'Browser source preview'}
       src={composedUrl}
       class="block h-64 w-full"
       sandbox="allow-scripts allow-same-origin"
      ></iframe>
     </div>
    {/if}
   </div>
  {/if}
 {/if}
</section>
