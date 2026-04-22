<script lang="ts">
 // Phase VI-β-7: 入力 Ingress パネル。
 //
 // 責務:
 //   - チャネル名・コンテンツ・is_final・flags・source・meta を組み立てて
 //     `POST /api/v1/control/ingress` に送信する。
 //   - snapshot から「既知のチャネル候補」を列挙（処理パイプラインの channel_to を集約）。
 //   - 直近入力を localStorage に保存して次回復元（content 本文は保存しない—プライバシー配慮）。
 //   - 送信履歴を直近 N 件保持して再送可能にする。
 //
 // 仕様メモ:
 //   - content は空文字を許容（is_final の区切りだけ送るユースケース）。
 //   - flags 入力はカンマ区切りテキスト。空要素は自動で除去。
 //   - source/meta は「詳細」折りたたみの中で入力。meta は JSON textarea。
 //   - WS の `channel_datum.pushed` イベントが届いた ID を緑でハイライトすれば送信結果
 //     がパイプラインに流れたと確認できるが、MVP では省略（EventStream で追える）。

 import { onMount } from 'svelte';
 import { api } from './api';
 import {
  ControlApiError,
  type DataSource,
  type IngressRequest,
  type IngressResponse,
 } from './types';

 // ---- フォーム state -------------------------------------------------------

 let channel = $state('');
 let content = $state('');
 let isFinal = $state(true);
 let flagsText = $state('');

 // source は空入力なら送らない（backend が既定値を補う）。
 let sourceKind = $state('');
 let sourceSubtype = $state('');
 let sourceActor = $state('');

 // meta は JSON textarea。空 or パース失敗時は送らない。
 let metaText = $state('');
 let metaError = $state<string | null>(null);

 let showAdvanced = $state(false);

 let busy = $state(false);
 let errorMsg = $state<string | null>(null);

 // ---- 送信履歴（直近 10 件）-----------------------------------------------

 type HistoryItem = {
  at: string;
  channel: string;
  contentPreview: string;
  isFinal: boolean;
  flags: string[];
  id: number | null;
  error: string | null;
 };

 let history = $state<HistoryItem[]>([]);

 /**
  * datalist に出す候補チャネル。
  * snapshot の ProcessorSummary には現状 channel_to が含まれていないので、ひとまず
  * これまで送信した履歴 + localStorage 記憶から拾う（サーバから channel_to を露出する拡張は将来）。
  */
 const channelSuggestions: string[] = $derived.by(() => {
  const all = [...loadStoredChannels(), ...history.map((h) => h.channel)];
  const uniq = all.filter((x, i) => x.trim().length > 0 && all.indexOf(x) === i);
  return uniq.sort();
 });

 // ---- localStorage I/O ----------------------------------------------------

 const STORAGE_KEY = 'vac.gui.ingress.v1';
 type Persisted = {
  lastChannel: string;
  channels: string[]; // 過去に使ったことのあるチャネル
 };

 function loadStoredChannels(): string[] {
  try {
   const raw = localStorage.getItem(STORAGE_KEY);
   if (!raw) return [];
   const p = JSON.parse(raw) as Partial<Persisted>;
   return Array.isArray(p.channels) ? p.channels.filter((x) => typeof x === 'string') : [];
  } catch {
   return [];
  }
 }

 function rememberChannel(c: string): void {
  if (!c) return;
  try {
   const raw = localStorage.getItem(STORAGE_KEY);
   const prev = raw ? (JSON.parse(raw) as Partial<Persisted>) : {};
   // 最新のものが末尾に来るよう、既存から c を除去して末尾に追加。
   const prevChannels = Array.isArray(prev.channels) ? prev.channels.filter((x) => x !== c) : [];
   const channels = [...prevChannels, c].slice(-20);
   const p: Persisted = { lastChannel: c, channels };
   localStorage.setItem(STORAGE_KEY, JSON.stringify(p));
  } catch {
   // localStorage 無効でも致命傷ではない
  }
 }

 function loadLastChannel(): string {
  try {
   const raw = localStorage.getItem(STORAGE_KEY);
   if (!raw) return '';
   const p = JSON.parse(raw) as Partial<Persisted>;
   return typeof p.lastChannel === 'string' ? p.lastChannel : '';
  } catch {
   return '';
  }
 }

 // ---- lifecycle -----------------------------------------------------------

 onMount(() => {
  channel = loadLastChannel();
 });

 // ---- ヘルパ --------------------------------------------------------------

 function parseFlags(text: string): string[] {
  return text
   .split(',')
   .map((s) => s.trim())
   .filter((s) => s.length > 0);
 }

 /** `metaText` を検証。戻り値は `{ obj } | null`（null = 空 or エラー）。 */
 function parseMeta(text: string): Record<string, unknown> | null {
  const t = text.trim();
  if (t === '') {
   metaError = null;
   return null;
  }
  try {
   const v = JSON.parse(t);
   if (typeof v !== 'object' || v === null || Array.isArray(v)) {
    metaError = 'meta は JSON object である必要があります（例: { "a": 1 }）';
    return null;
   }
   metaError = null;
   return v as Record<string, unknown>;
  } catch (e) {
   metaError = `JSON 解析エラー: ${e instanceof Error ? e.message : String(e)}`;
   return null;
  }
 }

 function buildSource(): DataSource | undefined {
  const kind = sourceKind.trim();
  const subtype = sourceSubtype.trim();
  const actor = sourceActor.trim();
  if (!kind && !subtype && !actor) return undefined;
  // kind が空なら既定値 `control.ingress` を入れる（actor だけ指定ケースを許容）
  return {
   kind: kind || 'control.ingress',
   subtype: subtype || null,
   actor: actor || null,
  };
 }

 async function submit(): Promise<void> {
  if (busy) return;
  errorMsg = null;

  const ch = channel.trim();
  if (!ch) {
   errorMsg = 'チャネルを入力してください';
   return;
  }

  // meta を先に検証（エラーなら送信しない）
  const metaObj = parseMeta(metaText);
  if (metaError) return;

  const flags = parseFlags(flagsText);
  const source = buildSource();

  const req: IngressRequest = {
   channel: ch,
   content,
   is_final: isFinal,
  };
  if (flags.length > 0) req.flags = flags;
  if (source) req.source = source;
  if (metaObj) req.meta = metaObj;

  busy = true;
  try {
   const res: IngressResponse = await api.ingress(req);
   rememberChannel(ch);
   pushHistory({
    at: new Date().toISOString(),
    channel: res.channel,
    contentPreview: previewOf(content),
    isFinal,
    flags,
    id: res.id,
    error: null,
   });
   // 連投しやすいよう content だけクリア、channel/flags/source/meta は残す
   content = '';
  } catch (e) {
   const msg = formatError(e);
   errorMsg = msg;
   pushHistory({
    at: new Date().toISOString(),
    channel: ch,
    contentPreview: previewOf(content),
    isFinal,
    flags,
    id: null,
    error: msg,
   });
  } finally {
   busy = false;
  }
 }

 function pushHistory(item: HistoryItem): void {
  // 先頭に追加、最大 10 件
  history = [item, ...history].slice(0, 10);
 }

 function previewOf(s: string): string {
  const t = s.replace(/\s+/g, ' ').trim();
  return t.length > 60 ? t.slice(0, 60) + '…' : t;
 }

 function formatError(e: unknown): string {
  if (e instanceof ControlApiError) {
   return `${e.status} ${e.statusText}: ${typeof e.body === 'string' ? e.body : JSON.stringify(e.body)}`;
  }
  return e instanceof Error ? e.message : String(e);
 }

 function resend(item: HistoryItem): void {
  // 履歴から content は取れない（preview だけ）ので、ユーザに再入力させる形にする。
  // channel/is_final/flags は復元する。
  channel = item.channel;
  isFinal = item.isFinal;
  flagsText = item.flags.join(', ');
  content = '';
  errorMsg = '本文を入力して再送してください（履歴には本文を保存していません）';
 }

 // Ctrl+Enter で送信（textarea 上でも効くように）
 function onContentKeydown(e: KeyboardEvent): void {
  if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
   e.preventDefault();
   void submit();
  }
 }
</script>

<section class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
 <header class="mb-3 flex items-center justify-between">
  <h3 class="text-sm font-semibold">Ingress (ChannelDatum 送信)</h3>
  <button
   type="button"
   class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-0.5 text-[10px] hover:bg-surface-200-800"
   onclick={() => (showAdvanced = !showAdvanced)}
   title="flags / source / meta の入力欄を表示"
  >
   {showAdvanced ? '◀ 詳細を隠す' : '▶ 詳細'}
  </button>
 </header>

 {#if errorMsg}
  <div class="mb-2 rounded bg-error-200-800 px-2 py-1 text-xs text-error-900-100">
   {errorMsg}
  </div>
 {/if}

 <!-- Channel -->
 <label class="mb-2 block text-xs">
  <span class="mb-1 block opacity-70">channel <span class="text-error-500">*</span></span>
  <input
   type="text"
   class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-sm"
   bind:value={channel}
   placeholder="例: main / twitch / voice"
   list="vac-ingress-channels"
   autocomplete="off"
  />
  <datalist id="vac-ingress-channels">
   {#each channelSuggestions as c (c)}
    <option value={c}></option>
   {/each}
  </datalist>
 </label>

 <!-- Content -->
 <label class="mb-2 block text-xs">
  <span class="mb-1 block opacity-70">content</span>
  <textarea
   class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-sm"
   rows="3"
   bind:value={content}
   onkeydown={onContentKeydown}
   placeholder="本文を入力（空でも送信可）。Ctrl+Enter で送信。"
  ></textarea>
 </label>

 <!-- is_final + 送信ボタン -->
 <div class="mb-2 flex items-center justify-between gap-2 text-xs">
  <label class="flex items-center gap-1">
   <input type="checkbox" bind:checked={isFinal} />
   <span>is_final</span>
  </label>
  <button
   type="button"
   class="rounded bg-primary-500 px-4 py-1 font-semibold text-primary-950 hover:bg-primary-400 disabled:opacity-50"
   onclick={() => void submit()}
   disabled={busy}
  >
   {busy ? '送信中…' : '送信 (Ctrl+Enter)'}
  </button>
 </div>

 <!-- 詳細 -->
 {#if showAdvanced}
  <div class="mt-3 space-y-2 rounded border border-dashed border-surface-300-700 p-2">
   <label class="block text-xs">
    <span class="mb-1 block opacity-70">flags (カンマ区切り)</span>
    <input
     type="text"
     class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
     bind:value={flagsText}
     placeholder="例: skip_ai, debug"
    />
   </label>

   <fieldset class="space-y-1 rounded border border-surface-300-700 p-2">
    <legend class="px-1 text-[10px] opacity-70">source（省略時 control.ingress / gui）</legend>
    <div class="grid grid-cols-3 gap-1 text-xs">
     <label>
      <span class="mb-0.5 block text-[10px] opacity-60">kind</span>
      <input
       type="text"
       class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-0.5 font-mono text-xs"
       bind:value={sourceKind}
       placeholder="user.web"
      />
     </label>
     <label>
      <span class="mb-0.5 block text-[10px] opacity-60">subtype</span>
      <input
       type="text"
       class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-0.5 font-mono text-xs"
       bind:value={sourceSubtype}
       placeholder="chat"
      />
     </label>
     <label>
      <span class="mb-0.5 block text-[10px] opacity-60">actor</span>
      <input
       type="text"
       class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-0.5 font-mono text-xs"
       bind:value={sourceActor}
       placeholder="alice"
      />
     </label>
    </div>
   </fieldset>

   <label class="block text-xs">
    <span class="mb-1 flex items-center justify-between opacity-70">
     <span>meta (JSON object)</span>
     {#if metaError}
      <span class="text-error-500 text-[10px]">{metaError}</span>
     {/if}
    </span>
    <textarea
     class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
     rows="3"
     bind:value={metaText}
     oninput={() => parseMeta(metaText)}
     placeholder={'{\n  "lang": "ja"\n}'}
    ></textarea>
   </label>
  </div>
 {/if}

 <!-- 履歴 -->
 {#if history.length > 0}
  <div class="mt-3 border-t border-surface-300-700 pt-2">
   <div class="mb-1 flex items-center justify-between text-[10px] opacity-70">
    <span>送信履歴（直近 {history.length} 件）</span>
    <button
     type="button"
     class="rounded border border-surface-300-700 bg-surface-50-950 px-1.5 py-0.5 hover:bg-surface-200-800"
     onclick={() => (history = [])}
    >
     クリア
    </button>
   </div>
   <ul class="space-y-1">
    {#each history as h, i (i + h.at)}
     <li
      class="flex items-center gap-2 rounded px-2 py-1 text-[11px] {h.error
       ? 'bg-error-200-800/40'
       : 'bg-surface-50-950'}"
     >
      <span class="shrink-0 font-mono opacity-60">
       {new Date(h.at).toLocaleTimeString('ja-JP', { hour12: false })}
      </span>
      <span class="shrink-0 rounded bg-surface-200-800 px-1.5 py-0.5 font-mono">
       {h.channel}
      </span>
      {#if h.id !== null}
       <span class="shrink-0 rounded bg-success-200-800 px-1 font-mono text-[10px] text-success-900-100">
        id={h.id}
       </span>
      {:else}
       <span class="shrink-0 rounded bg-error-500/40 px-1 font-mono text-[10px]">
        ERR
       </span>
      {/if}
      <span class="flex-1 truncate opacity-80" title={h.contentPreview}>
       {h.contentPreview || '(empty)'}
      </span>
      <button
       type="button"
       class="shrink-0 rounded border border-surface-300-700 px-1.5 py-0.5 hover:bg-surface-200-800"
       onclick={() => resend(h)}
       title="この channel / flags をフォームに戻す（本文は要再入力）"
      >
       ↻
      </button>
     </li>
    {/each}
   </ul>
  </div>
 {/if}
</section>
