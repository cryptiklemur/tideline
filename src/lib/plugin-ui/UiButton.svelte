<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiIcon from './UiIcon.svelte';
interface Props {
    node: Extract<UiNode, { kind: 'button' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
let cls = $derived(
    node.variant === 'primary' ? 'btn btn-primary btn-sm' :
    node.variant === 'warning' ? 'btn btn-warning btn-sm' :
    node.variant === 'ghost' ? 'btn btn-ghost btn-sm' :
    'btn btn-soft btn-sm'
);
</script>

<button
    class="{cls} gap-1"
    disabled={node.disabled}
    onclick={() => emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'click' } })}
>
    {#if node.icon}<UiIcon node={{ kind: 'icon', id: node.id + ':icon', icon: node.icon, size: 12 }} />{/if}
    {node.text}
</button>
