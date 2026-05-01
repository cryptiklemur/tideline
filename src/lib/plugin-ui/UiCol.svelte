<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    node: Extract<UiNode, { kind: 'col' }>;
    surfaceId: string;
    pluginId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, pluginId, emit }: Props = $props();
let gapClass = $derived(`gap-${node.gap ?? 2}`);
</script>

<div class="flex flex-col {gapClass}" data-col-id={node.id}>
    {#each node.children as child (child.id)}
        <UiTree node={child} {surfaceId} {pluginId} {emit} />
    {/each}
</div>
