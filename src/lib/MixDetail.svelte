<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { onMount } from 'svelte';
import Icon from './Icon.svelte';
import type { Mix, SinkInfo } from './types';

interface Props {
    mix: Mix;
    enabled: boolean;
    onUpdate: (patch: Partial<Mix>) => Promise<void>;
    onToggleEnabled: () => void;
    onDelete: () => void;
}

let { mix, enabled, onUpdate, onToggleEnabled, onDelete }: Props = $props();

let sinks = $state<SinkInfo[]>([]);
let sinksLoaded = $state(false);
let nameDraft = $state('');
let nameDirty = $derived(nameDraft.trim() !== mix.name && nameDraft.trim().length > 0);
let busy = $state(false);
let error = $state('');
let pendingDelete = $state(false);
let deleteTimer: ReturnType<typeof setTimeout> | null = null;
let lastMixId = '';

$effect(() => {
    if (mix.id !== lastMixId) {
        nameDraft = mix.name;
        lastMixId = mix.id;
    }
});

onMount(async () => {
    try {
        sinks = await invoke<SinkInfo[]>('list_sinks');
    } catch (e) {
        error = String(e);
    } finally {
        sinksLoaded = true;
    }
});

async function commitName() {
    const next = nameDraft.trim();
    if (!next || next === mix.name) { nameDraft = mix.name; return; }
    busy = true;
    error = '';
    try {
        await onUpdate({ name: next });
    } catch (e) {
        error = String(e);
        nameDraft = mix.name;
    } finally {
        busy = false;
    }
}

async function toggleSink(sinkName: string) {
    const next = mix.sinks.includes(sinkName)
        ? mix.sinks.filter(s => s !== sinkName)
        : [...mix.sinks, sinkName];
    busy = true;
    error = '';
    try {
        await onUpdate({ sinks: next });
    } catch (e) {
        error = String(e);
    } finally {
        busy = false;
    }
}

function requestDelete() {
    if (pendingDelete) {
        if (deleteTimer) { clearTimeout(deleteTimer); deleteTimer = null; }
        pendingDelete = false;
        onDelete();
        return;
    }
    pendingDelete = true;
    if (deleteTimer) clearTimeout(deleteTimer);
    deleteTimer = setTimeout(() => { pendingDelete = false; deleteTimer = null; }, 3000);
}

function onNameKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); commitName(); }
    if (e.key === 'Escape') { e.preventDefault(); nameDraft = mix.name; (e.target as HTMLInputElement).blur(); }
}
</script>

<div class="flex flex-col gap-3 max-w-[620px]">
    <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
        <div class="flex items-center justify-between gap-2">
            <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Name</span>
            <button
                class="inline-flex items-center gap-1.5 px-2.5 py-1 border rounded-full text-xs font-bold tracking-wider cursor-pointer transition-colors
                       {enabled
                         ? 'bg-primary/15 border-primary text-primary'
                         : 'bg-base-200 border-base-content/15 text-base-content/55 hover:bg-base-content/5 hover:text-base-content'}"
                aria-pressed={enabled}
                onclick={onToggleEnabled}
                title={enabled ? 'Disable mix' : 'Enable mix'}
            >
                <span class="w-1.5 h-1.5 rounded-full {enabled ? 'bg-primary shadow-[0_0_0_2px_var(--color-primary-content)]' : 'bg-base-content/55'}"></span>
                {enabled ? 'ENABLED' : 'DISABLED'}
            </button>
        </div>
        <div class="flex gap-2 items-center">
            <input
                bind:value={nameDraft}
                onkeydown={onNameKey}
                onblur={commitName}
                disabled={busy}
                aria-label="Mix name"
                class="input input-bordered flex-1"
            />
            {#if nameDirty}
                <button class="btn btn-primary btn-sm" onclick={commitName} disabled={busy}>Rename</button>
            {/if}
        </div>
    </section>

    <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
        <div class="flex items-center justify-between gap-2">
            <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Outputs</span>
            <span class="text-[10px] text-base-content/55 leading-snug">
                {mix.sinks.length === 0 ? 'No outputs picked' : mix.sinks.length === 1 ? '1 output' : `${mix.sinks.length} outputs`}
            </span>
        </div>
        {#if !sinksLoaded}
            <p class="m-0 text-[10px] text-base-content/55 leading-snug">Loading…</p>
        {:else if sinks.length === 0}
            <p class="m-0 text-[10px] text-base-content/55 leading-snug">No output sinks detected.</p>
        {:else}
            <ul class="list-none m-0 p-0 flex flex-col gap-1" role="group" aria-label="Mix outputs">
                {#each sinks as s (s.name)}
                    {@const checked = mix.sinks.includes(s.name)}
                    <li>
                        <button
                            class="w-full flex items-center gap-2.5 px-2.5 py-2 border rounded-md text-left cursor-pointer transition-colors min-w-0 disabled:opacity-55 disabled:cursor-not-allowed
                                   {checked
                                     ? 'bg-primary/15 border-primary text-base-content'
                                     : 'bg-base-200 border-base-content/15 text-base-content hover:bg-base-content/5 hover:border-base-content/25'}"
                            onclick={() => toggleSink(s.name)}
                            aria-pressed={checked}
                            disabled={busy}
                        >
                            <span class="w-4 h-4 border rounded flex items-center justify-center flex-shrink-0
                                         {checked
                                           ? 'bg-primary border-primary text-primary-content'
                                           : 'bg-base-100 border-base-content/15 text-primary'}" aria-hidden="true">
                                {#if checked}<Icon name="check" size={11} />{/if}
                            </span>
                            <span class="flex flex-col min-w-0 gap-px">
                                <span class="text-base text-base-content overflow-hidden text-ellipsis whitespace-nowrap">{s.description}</span>
                                <span class="font-mono text-xs text-base-content/55 overflow-hidden text-ellipsis whitespace-nowrap">{s.name}</span>
                            </span>
                        </button>
                    </li>
                {/each}
            </ul>
        {/if}
    </section>

    {#if error}<p class="m-0 text-[10px] text-error">{error}</p>{/if}

    <div class="flex gap-2">
        <button
            class="flex items-center justify-center gap-1.5 px-4 py-2.5 border rounded-md text-[10px] font-bold tracking-wider uppercase cursor-pointer transition-colors disabled:opacity-55 disabled:cursor-not-allowed
                   {pendingDelete ? 'bg-error border-error text-error-content' : 'bg-transparent border-base-content/15 text-base-content/55 hover:bg-error hover:text-error-content hover:border-error'}"
            onclick={requestDelete}
            disabled={busy}
            title={pendingDelete ? 'Click again to confirm' : 'Delete this mix'}
        >
            <Icon name="close" size={11} />
            {pendingDelete ? 'Confirm delete' : 'Delete mix'}
        </button>
    </div>
</div>
