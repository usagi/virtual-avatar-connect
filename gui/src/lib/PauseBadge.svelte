<script lang="ts">
 // 小さな「Active / Paused」バッジ。SnapshotView の各行や EventStream の pause_state 表示に使う。
 // paused=true で警告色、false で成功色、null で "不明" の中立色。

 interface Props {
  paused: boolean | null;
  /** Paused 時の追加説明（例: "moderator only"）。省略可。 */
  note?: string;
  /** 小サイズ。行内に埋め込むときに使う。 */
  compact?: boolean;
 }

 let { paused, note, compact = false }: Props = $props();

 const label = $derived(paused === null ? 'unknown' : paused ? 'paused' : 'active');
 const tone = $derived(
  paused === null
   ? 'bg-surface-300-700 text-surface-800-200'
   : paused
    ? 'bg-warning-200-800 text-warning-900-100'
    : 'bg-success-200-800 text-success-900-100'
 );
 const size = $derived(compact ? 'px-1.5 py-0 text-[10px]' : 'px-2 py-0.5 text-xs');
</script>

<span class="inline-flex items-center gap-1 rounded font-mono {tone} {size}">
 <span class="font-semibold uppercase tracking-wide">{label}</span>
 {#if note}
  <span class="opacity-75">· {note}</span>
 {/if}
</span>
