<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { onMount } from 'svelte';
import Icon from './Icon.svelte';
import Modal from './Modal.svelte';
import type { SourceInfo } from './types';

interface Props {
    existing: string[];
    excludeSink: string;
    onPick: (name: string) => void;
    onClose: () => void;
}

let { existing, excludeSink, onPick, onClose }: Props = $props();

let sources = $state<SourceInfo[]>([]);

onMount(async () => {
    const list = await invoke<SourceInfo[]>('list_sources');
    const ownMonitor = `${excludeSink}.monitor`;
    sources = list.filter(s => s.name !== ownMonitor);
});

function categoryOf(s: SourceInfo): 'input' | 'monitor' {
    return s.name.endsWith('.monitor') ? 'monitor' : 'input';
}

let groupedInputs = $derived(sources.filter(s => categoryOf(s) === 'input'));
let groupedMonitors = $derived(sources.filter(s => categoryOf(s) === 'monitor'));
</script>

{#snippet srcList(items: SourceInfo[])}
    <div class="flex flex-col gap-1 max-h-56 overflow-auto">
        {#each items as s (s.name)}
            <button
                class="grid grid-cols-[1fr_auto] grid-rows-[auto_auto] items-center gap-x-2.5 px-2.5 py-2 bg-base-100 border border-base-content/15 rounded-md text-left cursor-pointer transition-colors hover:enabled:bg-base-content/5 hover:enabled:border-primary disabled:opacity-55 disabled:cursor-not-allowed"
                disabled={existing.includes(s.name)}
                onclick={() => onPick(s.name)}
            >
                <span class="col-start-1 row-start-1 text-base font-medium truncate">{s.description}</span>
                <span class="col-start-1 row-start-2 font-mono text-sm text-base-content/55 truncate">{s.name}</span>
                {#if existing.includes(s.name)}
                    <span class="col-start-2 row-span-2 badge badge-primary badge-soft text-xs uppercase tracking-widest">added</span>
                {/if}
            </button>
        {/each}
    </div>
{/snippet}

<Modal label="Add source to channel" maxWidth="460px" {onClose}>
    <div class="flex items-center justify-between px-4 py-3 border-b border-base-content/10">
        <h2 class="text-lg font-semibold m-0">Add Source</h2>
        <button class="btn btn-ghost btn-square btn-sm" onclick={onClose} aria-label="Close" data-modal-close>
            <Icon name="close" size={14} />
        </button>
    </div>

    <section class="px-4 py-3 border-b border-base-content/10 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0">Hardware inputs</h3>
        {#if groupedInputs.length === 0}
            <p class="text-sm text-base-content/55 text-center py-2 m-0">No hardware inputs detected.</p>
        {:else}
            {@render srcList(groupedInputs)}
        {/if}
    </section>

    <section class="px-4 py-3 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0">App / channel monitors</h3>
        {#if groupedMonitors.length === 0}
            <p class="text-sm text-base-content/55 text-center py-2 m-0">No monitor sources available.</p>
        {:else}
            {@render srcList(groupedMonitors)}
        {/if}
    </section>
</Modal>
