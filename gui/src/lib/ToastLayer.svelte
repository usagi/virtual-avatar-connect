<script lang="ts">
 /**
  * トーストレイヤ（右下）。`toastStore.items` を購読するだけのシンプルな表示。
  *
  * Control イベントと紐付けるブリッジは `ToastBridge.svelte` で別管理し、
  * 本コンポーネントは純粋な描画層とする（テストしやすさのため）。
  */
 import { toastStore, type ToastMessage } from './toasts.svelte';

 function toneClass(t: ToastMessage['tone']): string {
  switch (t) {
   case 'info':
    return 'bg-primary-500/15 text-primary-900-100 border-primary-500/40';
   case 'success':
    return 'bg-success-500/15 text-success-900-100 border-success-500/40';
   case 'warn':
    return 'bg-warning-500/15 text-warning-900-100 border-warning-500/40';
   case 'error':
    return 'bg-error-500/15 text-error-900-100 border-error-500/40';
  }
 }
</script>

<div
 class="pointer-events-none fixed bottom-8 right-4 z-50 flex w-80 max-w-[90vw] flex-col gap-2"
 role="region"
 aria-label="Notifications"
 aria-live="polite"
>
 {#each toastStore.items as t (t.id)}
  <div
   class="pointer-events-auto rounded border px-3 py-2 text-sm shadow-lg {toneClass(t.tone)}"
   role="status"
  >
   <div class="flex items-start justify-between gap-2">
    <div class="font-semibold">{t.title}</div>
    <button
     type="button"
     class="-mr-1 -mt-0.5 text-lg leading-none opacity-70 hover:opacity-100"
     aria-label="Dismiss"
     onclick={() => toastStore.dismiss(t.id)}
    >
     ×
    </button>
   </div>
   {#if t.detail}
    <div class="mt-0.5 whitespace-pre-wrap break-words text-xs opacity-80">{t.detail}</div>
   {/if}
   {#if t.action}
    <div class="mt-1.5 flex justify-end">
     <button
      type="button"
      class="rounded border border-current/40 px-2 py-0.5 text-xs font-medium hover:bg-current/10"
      onclick={() => toastStore.triggerAction(t.id)}
     >
      {t.action.label}
     </button>
    </div>
   {/if}
  </div>
 {/each}
</div>
