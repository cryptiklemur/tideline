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
</script>

<section class="flex flex-col gap-2 mb-4" data-section-id={node.id}>
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
