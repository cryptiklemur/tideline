<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiIcon from './UiIcon.svelte';
interface Props {
    node: Extract<UiNode, { kind: 'list' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
let order: string[] = $state([]);
$effect(() => { order = node.items.map((i) => i.id); });

function move(idx: number, dir: -1 | 1) {
    const next = [...order];
    const target = idx + dir;
    if (target < 0 || target >= next.length) return;
    [next[idx], next[target]] = [next[target], next[idx]];
    order = next;
    emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'order', value: next } });
}
let byId = $derived(new Map(node.items.map((i) => [i.id, i])));
</script>

<ul class="flex flex-col gap-1 border border-base-content/10 rounded p-2 bg-base-300/40">
    {#each order as id, idx (id)}
        {@const item = byId.get(id)}
        {#if item}
            <li class="flex items-center gap-2 text-sm">
                {#if item.icon}<UiIcon node={{ kind: 'icon', id: id + ':icon', icon: item.icon, size: 12 }} />{/if}
                <span class="flex-1">{item.label}</span>
                {#if node.sortable}
                    <button class="btn btn-ghost btn-xs" onclick={() => move(idx, -1)} aria-label="Move up">up</button>
                    <button class="btn btn-ghost btn-xs" onclick={() => move(idx, 1)} aria-label="Move down">down</button>
                {/if}
            </li>
        {/if}
    {/each}
</ul>
