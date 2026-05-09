<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiIcon from './UiIcon.svelte';
interface Props {
    node: Extract<UiNode, { kind: 'button' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
let isIconOnly = $derived(!!node.icon && (!node.text || node.text.length === 0));
let cls = $derived(
    (node.variant === 'primary' ? 'btn btn-primary btn-sm' :
        node.variant === 'warning' ? 'btn btn-warning btn-sm' :
        node.variant === 'success' ? 'btn btn-success btn-soft btn-sm' :
        node.variant === 'ghost' ? 'btn btn-ghost btn-sm' :
        'btn btn-soft btn-sm') + (isIconOnly ? ' btn-square' : '')
);
</script>

<button
    class="{cls} gap-1"
    disabled={node.disabled}
    title={node.tooltip ?? undefined}
    onclick={() => emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'click' } })}
>
    {#if node.icon}<UiIcon node={{ kind: 'icon', id: node.id + ':icon', icon: node.icon, size: 12 }} />{/if}
    {node.text}
</button>
