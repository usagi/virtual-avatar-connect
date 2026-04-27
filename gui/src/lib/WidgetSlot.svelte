<script lang="ts">
 /**
  * Lightweight placeholder shell for planned dashboard widgets.
  *
  * The component gives unfinished resource, settings, and utility surfaces a
  * consistent frame until a concrete widget replaces the placeholder content.
  */
 import type { Snippet } from 'svelte';

 interface Props {
  /** Card title shown in the header. */
  title?: string;
  /** Small header note, usually status or scope. */
  subtitle?: string;
  /** Optional real widget content. */
  children?: Snippet;
  /** Render without the dashed card frame. */
  compact?: boolean;
  /** Text shown while no widget content is mounted. */
  placeholder?: string;
  /** Optional region id for links and tests. */
  id?: string;
 }

 const {
  title,
  subtitle,
  children,
  compact = false,
  placeholder = 'This widget slot is reserved for a planned GUI surface.',
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
