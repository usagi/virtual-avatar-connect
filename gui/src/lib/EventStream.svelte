<script lang="ts">
 // eventsStore.recent をリアルタイムに表示するログペイン。
 //
 // 機能:
 //   - kind ごとのフィルタ（heartbeat はデフォルト OFF、その他 ON）
 //   - kind ごとの色分け
 //   - 1 行サマリ + 展開で生 JSON
 //   - Clear（バッファクリア）、Pause（UI 更新停止 — WS は開いたまま）
 //   - dropped（lagged）・received カウンタ表示
 //
 // 設計ポイント:
 //   - **型安全** : 各 event の 1 行サマリ生成を `switch (ev.kind)` + `assertNever` で書く。
 //     Rust 側で ControlEvent variant を追加したとき、`types.ts` の更新とここの更新が
 //     **両方**必要になるよう、compile-time に検出される。

 import { eventsStore, type TimestampedEvent } from './events.svelte';
 import { assertNever, type ControlEvent, type ControlEventKind } from './types';

 type FilterMap = Record<ControlEventKind, boolean>;

 // heartbeat はノイズになるのでデフォルトでは非表示。他は全部表示。
let filter: FilterMap = $state({
 channel_datum: true,
 heartbeat: false,
 pause_state: true,
 reloaded: true,
 oauth_status: true,
 lagged: true,
 // Phase VI-β-8: Pipeline ノードの発火ログ。パイプライン・ビューと併せて確認するときに便利なので
 // デフォルト ON。ノイズに感じたらこのチェックを外せば一覧からだけ隠れる。
 processor_invoked: true,
 // Phase VI-γ-1: 再起動通知。ノイズになる頻度ではないのでデフォルト ON。
 restarting: true,
 // Phase VI-γ-2b: Managed App の状態変化。配信中に OBS 等の起動/停止を追うのに便利なのでデフォルト ON。
 managed_app_state: true,
 // Phase δ-6: Flowgraph ホットリロード通知。編集中はあったほうが便利なのでデフォルト ON。
 flowgraph_reloaded: true,
});

 let paused: boolean = $state(false);
 let expandedSeq: number | null = $state(null);

 // paused=false のときだけ store を読む。paused=true の間は frozen 状態を保持したい。
 let frozen: TimestampedEvent[] = $state([]);

 $effect(() => {
  if (!paused) {
   // 追従中は store の配列を参照（リアクティブに更新）
   frozen = eventsStore.recent;
  }
 });

 const visible = $derived(
  // 新しいイベントを上に。filter で絞る。
  [...frozen].reverse().filter((e) => filter[e.event.kind])
 );

 function toggleFilter(k: ControlEventKind): void {
  filter = { ...filter, [k]: !filter[k] };
 }

 function toggleExpand(seq: number): void {
  expandedSeq = expandedSeq === seq ? null : seq;
 }

 /** 1 行サマリ文字列。Rust の ControlEvent variant が追加されたら compile error になる。 */
 function summarize(ev: ControlEvent): string {
  switch (ev.kind) {
   case 'channel_datum':
    return `[${ev.phase}] #${ev.id} @${ev.channel}: ${truncate(ev.content, 120)}`;
   case 'lagged':
    return `dropped ${ev.dropped} events`;
   case 'heartbeat':
    return `heartbeat @ ${ev.now}`;
   case 'pause_state':
    return `${ev.paused ? 'PAUSED' : 'RESUMED'} — target=${ev.target}, procs=[${ev.processors_affected.join(',')}], ais=[${ev.ais_affected.join(',')}]`;
   case 'reloaded':
    return `target=${ev.target}${ev.id ? ` id=${ev.id}` : ''}`;
  case 'oauth_status':
   return `${ev.account} → ${ev.status} (code=${ev.view.user_code})`;
  case 'processor_invoked': {
   const idPart = ev.id ? `#${ev.id}` : `[${ev.index}]`;
   return `${ev.feature}${idPart} ← @${ev.trigger_channel} (cd=#${ev.channel_datum_id}, ${ev.elapsed_ms}ms, ${ev.outcome})`;
  }
  case 'restarting':
   return `restarting → pid=${ev.new_pid}, conf=${ev.new_conf_path} (graceful=${ev.graceful_ms}ms)`;
  case 'managed_app_state':
   return `${ev.id} ${ev.running ? 'RUNNING' : 'STOPPED'} pids=[${ev.pids.join(',')}]`;
  case 'flowgraph_reloaded':
   return `${ev.root_dir} ok=${ev.ok} nodes=${ev.node_count} (err=${ev.error_count} warn=${ev.warning_count})`;
  default:
   return assertNever(ev);
 }
}

 /** Tailwind の色クラス。kind に応じた帯色。 */
 function kindTone(k: ControlEventKind): string {
  switch (k) {
   case 'channel_datum':
    return 'bg-primary-200-800 text-primary-900-100';
   case 'heartbeat':
    return 'bg-surface-300-700 text-surface-800-200';
   case 'pause_state':
    return 'bg-warning-200-800 text-warning-900-100';
   case 'reloaded':
    return 'bg-success-200-800 text-success-900-100';
   case 'oauth_status':
    return 'bg-tertiary-200-800 text-tertiary-900-100';
  case 'lagged':
   return 'bg-error-200-800 text-error-900-100';
  case 'processor_invoked':
   return 'bg-secondary-200-800 text-secondary-900-100';
  case 'restarting':
   return 'bg-warning-200-800 text-warning-900-100';
  case 'managed_app_state':
   return 'bg-tertiary-200-800 text-tertiary-900-100';
  case 'flowgraph_reloaded':
   return 'bg-success-200-800 text-success-900-100';
  default:
   return assertNever(k);
 }
}

 function truncate(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n)}…` : s;
 }

 function formatTime(ms: number): string {
  const d = new Date(ms);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  const ms3 = String(d.getMilliseconds()).padStart(3, '0');
  return `${hh}:${mm}:${ss}.${ms3}`;
 }

 // eventsStore への参照（テンプレで直接読めるように $derived で露出）
 const received = $derived(eventsStore.received_count);
 const dropped = $derived(eventsStore.dropped_count);

const ALL_KINDS: readonly ControlEventKind[] = [
 'channel_datum',
 'processor_invoked',
 'pause_state',
 'reloaded',
 'oauth_status',
 'restarting',
 'managed_app_state',
 'flowgraph_reloaded',
 'heartbeat',
 'lagged',
];
</script>

<section class="flex h-full flex-col space-y-2">
 <header class="flex items-center justify-between">
  <h2 class="text-lg font-semibold">
   ライブイベント
   <span class="text-xs font-normal opacity-70">
    (received: {received.toLocaleString()}, dropped: {dropped.toLocaleString()})
   </span>
  </h2>
  <div class="flex items-center gap-2 text-xs">
   <button
    type="button"
    class="rounded border border-surface-300-700 bg-surface-100-900 px-2 py-0.5 hover:bg-surface-200-800"
    onclick={() => (paused = !paused)}
    title={paused ? 'UI の更新を再開（WS は止まっていません）' : 'UI の更新を一時停止（スクロールの追従を止めます）'}
   >
    {paused ? '▶ 再開' : '⏸ 一時停止'}
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 bg-surface-100-900 px-2 py-0.5 hover:bg-surface-200-800"
    onclick={() => eventsStore.clearRecent()}
   >
    クリア
   </button>
  </div>
 </header>

 <div class="flex flex-wrap gap-1 text-xs">
  {#each ALL_KINDS as k (k)}
   <button
    type="button"
    class="rounded px-2 py-0.5 font-mono transition {filter[k]
     ? kindTone(k)
     : 'bg-surface-200-800 text-surface-700-300 opacity-50'}"
    onclick={() => toggleFilter(k)}
   >
    {k}
   </button>
  {/each}
 </div>

 <div
  class="flex-1 overflow-y-auto rounded-lg border border-surface-300-700 bg-surface-50-950"
  style="min-height: 24rem; max-height: 32rem;"
 >
  {#if visible.length === 0}
   <p class="p-4 text-center text-xs opacity-60">
    {#if frozen.length === 0}
     まだイベントが届いていません。VAC からのメッセージを待機中…
    {:else}
     フィルタで全て除外されています。上のタグをクリックして有効化してください。
    {/if}
   </p>
  {:else}
   <ul class="divide-y divide-surface-200-800/60 text-xs">
    {#each visible as item (item.seq)}
     <li class="px-2 py-1 hover:bg-surface-100-900/50">
      <button
       type="button"
       class="flex w-full items-start gap-2 text-left"
       onclick={() => toggleExpand(item.seq)}
      >
       <span class="font-mono opacity-60">{formatTime(item.received_at)}</span>
       <span class="rounded px-1.5 py-0 font-mono text-[10px] font-semibold uppercase {kindTone(item.event.kind)}">
        {item.event.kind}
       </span>
       <span class="flex-1 font-mono break-all">{summarize(item.event)}</span>
      </button>
      {#if expandedSeq === item.seq}
       <pre
        class="mt-1 overflow-x-auto rounded bg-surface-200-800/60 p-2 text-[11px] leading-tight font-mono"
       >{JSON.stringify(item.event, null, 2)}</pre>
      {/if}
     </li>
    {/each}
   </ul>
  {/if}
 </div>
</section>
