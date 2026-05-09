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
let isDraggable = $derived(!!node.draggable);
let isDropTarget = $derived(!!node.drop_group);

let isDragging = $state(false);
let isDragOver = $state(false);

function onDragStart(ev: DragEvent) {
    if (!isDraggable || !node.drop_group) return;
    ev.dataTransfer?.setData('application/x-tideline-row', JSON.stringify({
        id: node.id,
        drop_group: node.drop_group,
    }));
    if (ev.dataTransfer) ev.dataTransfer.effectAllowed = 'move';
    isDragging = true;
}

function onDragEnd() {
    isDragging = false;
}

function onDragOver(ev: DragEvent) {
    if (!isDropTarget) return;
    const types = ev.dataTransfer?.types;
    if (!types || !Array.from(types).includes('application/x-tideline-row')) return;
    ev.preventDefault();
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'move';
    isDragOver = true;
}

function onDragLeave() {
    isDragOver = false;
}

function onDrop(ev: DragEvent) {
    if (!isDropTarget) return;
    const raw = ev.dataTransfer?.getData('application/x-tideline-row');
    if (!raw) return;
    let payload: { id: string; drop_group: string };
    try {
        payload = JSON.parse(raw);
    } catch {
        return;
    }
    if (payload.drop_group !== node.drop_group) return;
    if (payload.id === node.id) {
        isDragOver = false;
        return;
    }
    ev.preventDefault();
    isDragOver = false;
    emit({
        surface_id: surfaceId,
        node_id: node.id,
        value: { type: 'drop', from_id: payload.id },
    });
}
</script>

<div
    class="flex flex-row {alignClass} {gapClass} {padClass} {cardClass} {mutedClass}
        {isDraggable ? 'cursor-grab active:cursor-grabbing' : ''}
        {isDragging ? 'opacity-40' : ''}
        {isDragOver ? 'ring-2 ring-primary/60 ring-offset-1 ring-offset-base-100' : ''}"
    data-row-id={node.id}
    draggable={isDraggable}
    role={isDraggable || isDropTarget ? 'listitem' : undefined}
    ondragstart={onDragStart}
    ondragend={onDragEnd}
    ondragover={onDragOver}
    ondragleave={onDragLeave}
    ondrop={onDrop}
>
    {#each node.children as child (child.id)}
        <UiTree node={child} {surfaceId} {pluginId} {emit} />
    {/each}
</div>
