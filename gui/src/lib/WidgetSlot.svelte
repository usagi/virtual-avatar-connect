<script lang="ts">
 /**
  * Phase VI-γ-1: ウィジェット配置用のスロット土台。
  *
  * γ-1 では **見た目としての領域** だけ提供し、ウィジェット自体（OBS 連携、`!bsr` キュー、
  * Twitch チャット等）は γ-2 以降で個別に作り込んで差し込む。ここでは API だけ先に固める:
  *
  *   - `title` を指定するとボーダー付きのカードで枠を持つ
  *   - 空スロットのときは "この領域にウィジェットを配置できます" のプレースホルダ
  *   - `compact` で枠線なし・余白最小の variant（ステータスバー近くに並べるとき用）
  *
  * ここで作った interface は γ-2 以降の実ウィジェットを <WidgetSlot>...</WidgetSlot> で包むか、
  * あるいは slot 経由で注入する方式のどちらでも扱えるようにしている。
  */
 import type { Snippet } from 'svelte';

 interface Props {
  /** カード左上に小さく出す見出し。省略可。 */
  title?: string;
  /** 右上に出す副情報（プロフィール名など）。省略可。 */
  subtitle?: string;
  /** 子要素。未指定時はプレースホルダを出す。 */
  children?: Snippet;
  /** 枠・余白を最小化する。 */
  compact?: boolean;
  /** プレースホルダに出す説明文。未指定時は定形文。 */
  placeholder?: string;
  /** ref 用の id（URL フラグメントや a11y 用）。 */
  id?: string;
 }

 const {
  title,
  subtitle,
  children,
  compact = false,
  placeholder = 'このスロットにウィジェットを配置できます（γ-2 以降で実装）。',
  id,
 }: Props = $props();
</script>

{#if compact}
 <div
  class="flex flex-col"
  {id}
  role="region"
  aria-label={title}
 >
  {#if children}
   {@render children()}
  {:else}
   <p class="px-2 py-1 text-xs opacity-60">{placeholder}</p>
  {/if}
 </div>
{:else}
 <section
  class="rounded-lg border border-dashed border-surface-300-700 bg-surface-100-900 p-4"
  {id}
  aria-label={title}
 >
  {#if title || subtitle}
   <header class="mb-2 flex items-baseline justify-between gap-2">
    {#if title}
     <h3 class="text-sm font-semibold opacity-80">{title}</h3>
    {/if}
    {#if subtitle}
     <span class="text-xs opacity-60">{subtitle}</span>
    {/if}
   </header>
  {/if}
  {#if children}
   {@render children()}
  {:else}
   <p class="text-xs opacity-60">{placeholder}</p>
  {/if}
 </section>
{/if}
