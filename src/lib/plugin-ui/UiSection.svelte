<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    node: Extract<UiNode, { kind: 'section' }>;
    surfaceId: string;
    pluginId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, pluginId, emit }: Props = $props();

let isCard = $derived(node.variant === 'card');
let gap = $derived(node.gap ?? (isCard ? 3 : 2));
let gapClass = $derived(
    gap >= 4 ? 'gap-4' : gap === 3 ? 'gap-3' : gap === 2 ? 'gap-2' : gap === 1 ? 'gap-1' : 'gap-0'
);
let containerClass = $derived(
    isCard
        ? `flex flex-col ${gapClass} mb-4 rounded-lg border border-base-content/10 bg-base-200/40 px-4 py-3`
        : `flex flex-col ${gapClass} mb-4`
);
</script>

<section class={containerClass} data-section-id={node.id}>
    {#if node.title}
        <h4 class="text-[10px] font-bold uppercase tracking-widest text-base-content/55 m-0">{node.title}</h4>
    {/if}
    {#if node.subtitle}
        <p class="text-xs text-base-content/55 m-0 leading-snug">{node.subtitle}</p>
    {/if}
    {#each node.children as child (child.id)}
        <UiTree node={child} {surfaceId} {pluginId} {emit} />
    {/each}
</section>
