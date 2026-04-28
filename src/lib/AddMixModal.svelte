<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { onMount, tick } from 'svelte';
import Icon from './Icon.svelte';
import Modal from './Modal.svelte';
import type { SinkInfo } from './types';

interface Props {
    existingNames: string[];
    onCreate: (name: string, sinks: string[]) => Promise<void>;
    onClose: () => void;
}

let { existingNames, onCreate, onClose }: Props = $props();

let name = $state('');
let selectedSinks = $state<string[]>([]);
let sinks = $state<SinkInfo[]>([]);
let sinksLoaded = $state(false);
let busy = $state(false);
let submitError = $state('');
let nameTouched = $state(false);
let submitAttempted = $state(false);
let nameInputEl = $state<HTMLInputElement>();

let nameError = $derived.by(() => {
    const trimmed = name.trim();
    if (!trimmed) return 'Name is required';
    if (existingNames.includes(trimmed)) return 'A mix with that name already exists';
    return '';
});
let showNameError = $derived((nameTouched || submitAttempted) && !!nameError);
let canSubmit = $derived(!busy && !nameError);

onMount(async () => {
    try {
        sinks = await invoke<SinkInfo[]>('list_sinks');
    } catch {
        sinks = [];
    } finally {
        sinksLoaded = true;
    }
    await tick();
    nameInputEl?.focus();
});

function toggleSink(sinkName: string) {
    if (selectedSinks.includes(sinkName)) {
        selectedSinks = selectedSinks.filter(s => s !== sinkName);
    } else {
        selectedSinks = [...selectedSinks, sinkName];
    }
}

async function commit() {
    submitAttempted = true;
    if (nameError) {
        nameInputEl?.focus();
        return;
    }
    submitError = '';
    busy = true;
    try {
        await onCreate(name.trim(), selectedSinks);
        onClose();
    } catch (e) {
        submitError = String(e);
    } finally {
        busy = false;
    }
}

function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.target as HTMLElement)?.tagName === 'INPUT') {
        e.preventDefault();
        commit();
    }
}
</script>

<Modal label="Add mix" maxWidth="520px" {onClose}>
    <div class="flex items-center justify-between px-4 py-3 border-b border-base-content/10">
        <h2 class="text-lg font-semibold m-0">Add Mix</h2>
        <button class="btn btn-ghost btn-square btn-sm" onclick={onClose} aria-label="Close" data-modal-close>
            <Icon name="close" size={14} />
        </button>
    </div>

    <section class="px-4 py-3 border-b border-base-content/10 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0 flex items-center gap-1">
            <span>Name</span>
            <span class="text-error" aria-hidden="true">*</span>
        </h3>
        <input
            bind:this={nameInputEl}
            bind:value={name}
            placeholder="e.g. Stream, Headphones, Speakers"
            onkeydown={onKey}
            onblur={() => nameTouched = true}
            disabled={busy}
            class="input input-bordered w-full {showNameError ? 'input-error' : ''}"
            aria-invalid={showNameError}
            aria-describedby={showNameError ? 'mix-name-err' : undefined}
        />
        {#if showNameError}
            <p class="text-error text-sm flex items-center gap-1.5 m-0" id="mix-name-err" role="alert">
                <Icon name="alert" size={12} />
                <span>{nameError}</span>
            </p>
        {/if}
    </section>

    <section class="px-4 py-3 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0">Outputs</h3>
        <p class="text-sm text-base-content/55 m-0 leading-snug">Pick the physical sinks this mix should play to. You can change this later.</p>
        {#if !sinksLoaded}
            <p class="text-sm text-base-content/55 m-0">Loading…</p>
        {:else if sinks.length === 0}
            <p class="text-sm text-error m-0">No output sinks detected.</p>
        {:else}
            <ul class="list-none m-0 p-0 flex flex-col gap-1 max-h-72 overflow-y-auto" role="group" aria-label="Mix outputs">
                {#each sinks as s (s.name)}
                    {@const checked = selectedSinks.includes(s.name)}
                    <li>
                        <button
                            class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-md border text-left cursor-pointer transition-colors disabled:opacity-55 disabled:cursor-not-allowed min-w-0
                                   {checked ? 'bg-primary/15 border-primary' : 'bg-base-100 border-base-content/15 hover:bg-base-content/5 hover:border-base-content/25'}"
                            onclick={() => toggleSink(s.name)}
                            aria-pressed={checked}
                            disabled={busy}
                        >
                            <span
                                class="w-4 h-4 rounded flex items-center justify-center flex-shrink-0 border
                                       {checked ? 'bg-primary border-primary text-primary-content' : 'bg-base-100 border-base-content/20 text-primary'}"
                                aria-hidden="true"
                            >
                                {#if checked}<Icon name="check" size={11} />{/if}
                            </span>
                            <span class="flex flex-col min-w-0 gap-px">
                                <span class="text-base truncate">{s.description}</span>
                                <span class="font-mono text-xs text-base-content/55 truncate">{s.name}</span>
                            </span>
                        </button>
                    </li>
                {/each}
            </ul>
        {/if}
    </section>

    {#if submitError}
        <div class="alert alert-error alert-soft mx-4 py-2" role="alert">
            <Icon name="alert" size={14} />
            <span class="text-sm">{submitError}</span>
        </div>
    {/if}

    <div class="flex gap-2 px-4 py-3 border-t border-base-content/10">
        <button class="btn btn-soft flex-1" onclick={onClose} disabled={busy}>Cancel</button>
        <button
            class="btn btn-primary flex-1"
            onclick={commit}
            disabled={!canSubmit}
            aria-busy={busy}
        >
            {#if busy}
                <span class="inline-flex animate-spin"><Icon name="loader" size={14} /></span>
                <span>Adding…</span>
            {:else}
                <span>Add Mix</span>
            {/if}
        </button>
    </div>
</Modal>
