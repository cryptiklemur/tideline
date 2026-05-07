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
let padClass = $derived(node.pad !== undefined ? `p-${node.pad}` : '');
let cardClass = $derived(
    node.variant === 'card'
        ? 'rounded-lg border border-base-content/10 bg-base-100 transition-colors hover:border-base-content/25'
        : ''
);
let mutedClass = $derived(node.muted ? 'opacity-50' : '');
</script>

<div
    class="flex flex-row {alignClass} {gapClass} {padClass} {cardClass} {mutedClass}"
    data-row-id={node.id}
>
    {#each node.children as child (child.id)}
        <UiTree node={child} {surfaceId} {pluginId} {emit} />
    {/each}
</div>
