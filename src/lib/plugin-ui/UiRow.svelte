<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    node: Extract<UiNode, { kind: 'row' }>;
    surfaceId: string;
    pluginId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, pluginId, emit }: Props = $props();
let alignClass = $derived(node.align === 'center' ? 'items-center' : node.align === 'end' ? 'items-end' : 'items-start');
let gapClass = $derived(`gap-${node.gap ?? 2}`);
</script>

<div class="flex flex-row {alignClass} {gapClass}" data-row-id={node.id}>
    {#each node.children as child (child.id)}
        <UiTree node={child} {surfaceId} {pluginId} {emit} />
    {/each}
</div>
