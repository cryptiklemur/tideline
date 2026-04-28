<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { onMount } from 'svelte';
import Icon from './Icon.svelte';
import Modal from './Modal.svelte';
import type { RunningApp } from './types';

interface Props {
    existing: string[];
    onPick: (binary: string) => void;
    onClose: () => void;
}

let { existing, onPick, onClose }: Props = $props();

let apps = $state<RunningApp[]>([]);
let manual = $state('');

onMount(async () => {
    apps = await invoke<RunningApp[]>('list_running_apps');
});

function pickManual() {
    const v = manual.trim();
    if (!v) return;
    onPick(v);
}

function onManualKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); pickManual(); }
}
</script>

<Modal label="Add program to channel" maxWidth="420px" {onClose}>
    <div class="flex items-center justify-between px-4 py-3 border-b border-base-content/10">
        <h2 class="text-lg font-semibold m-0">Add Program</h2>
        <button class="btn btn-ghost btn-square btn-sm" onclick={onClose} aria-label="Close" data-modal-close>
            <Icon name="close" size={14} />
        </button>
    </div>

    <section class="px-4 py-3 border-b border-base-content/10 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0">Currently playing</h3>
        {#if apps.length === 0}
            <p class="text-sm text-base-content/55 text-center py-3 m-0">No apps currently playing audio.</p>
        {:else}
            <div class="flex flex-col gap-1 max-h-72 overflow-auto">
                {#each apps as app (app.binary)}
                    <button
                        class="grid grid-cols-[1fr_auto_auto] items-center gap-2.5 px-2.5 py-2 bg-base-100 border border-base-content/15 rounded-md text-left cursor-pointer transition-colors hover:enabled:bg-base-content/5 hover:enabled:border-primary disabled:opacity-55 disabled:cursor-not-allowed"
                        disabled={existing.includes(app.binary)}
                        onclick={() => onPick(app.binary)}
                    >
                        <span class="font-medium truncate">{app.application_name}</span>
                        <span class="font-mono text-sm text-base-content/55">{app.binary}</span>
                        {#if existing.includes(app.binary)}
                            <span class="badge badge-primary badge-soft text-xs uppercase tracking-widest">added</span>
                        {/if}
                    </button>
                {/each}
            </div>
        {/if}
    </section>

    <section class="px-4 py-3 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0">Manual entry</h3>
        <p class="text-sm text-base-content/55 m-0 leading-snug">
            Enter the binary name (e.g. <span class="font-mono">firefox</span>, <span class="font-mono">discord</span>) for an app that isn't running yet.
        </p>
        <div class="flex gap-1.5">
            <input
                bind:value={manual}
                placeholder="binary name"
                onkeydown={onManualKey}
                class="input input-bordered flex-1 font-mono"
            />
            <button class="btn btn-primary" onclick={pickManual} disabled={!manual.trim()}>Add</button>
        </div>
    </section>
</Modal>
