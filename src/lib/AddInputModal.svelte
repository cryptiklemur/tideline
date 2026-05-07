<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { onMount, tick } from 'svelte';
import Icon from './Icon.svelte';
import Modal from './Modal.svelte';
import type { SourceInfo } from './types';
import { pluginUi } from './plugin-ui/pluginUi.svelte';
import InputOverlay from './plugin-ui/InputOverlay.svelte';
import type { InputOverlayContribution, UiEvent } from './plugin-ui/types';

type InputKind = 'input' | 'physical_input';

interface Props {
    existingNames: string[];
    onCreate: (kind: InputKind, name: string, physicalSource: string) => Promise<void>;
    onClose: () => void;
}

let { existingNames, onCreate, onClose }: Props = $props();

let kind = $state<InputKind>('input');
let name = $state('');
let physSource = $state('');
let hardwareInputs = $state<SourceInfo[]>([]);
let hardwareInputsLoadError = $state('');
let hardwareInputsLoaded = $state(false);
let busy = $state(false);
let submitError = $state('');
let nameTouched = $state(false);
let physTouched = $state(false);
let submitAttempted = $state(false);
let nameInputEl = $state<HTMLInputElement>();

let inputOverlays = $derived(
    pluginUi.contributions.input_overlays.filter(o =>
        o.input_filter.kind === 'all'
        || o.input_filter.kind === 'physical_only'
        || (o.input_filter.kind === 'source_names' && o.input_filter.names.includes(physSource))
    )
);

function emitOverlay(pluginId: string, ev: UiEvent) {
    pluginUi.emit(pluginId, ev);
}

function deriveNameFromSource(srcName: string): string {
    const src = hardwareInputs.find(s => s.name === srcName);
    return src ? src.description : srcName;
}

let effectiveName = $derived.by(() => {
    const trimmed = name.trim();
    if (trimmed) return trimmed;
    if (kind === 'physical_input' && physSource) return deriveNameFromSource(physSource);
    return '';
});

let nameError = $derived.by(() => {
    if (kind === 'input' && !name.trim()) return 'Name is required';
    if (effectiveName && existingNames.includes(effectiveName)) {
        return 'A channel with that name already exists';
    }
    return '';
});

let physError = $derived.by(() => {
    if (kind === 'physical_input' && !physSource) return 'Pick a hardware input';
    return '';
});

let showNameError = $derived((nameTouched || submitAttempted) && !!nameError);
let showPhysError = $derived((physTouched || submitAttempted) && !!physError);
let canSubmit = $derived(!busy && !nameError && !physError);

onMount(async () => {
    try {
        hardwareInputs = await invoke<SourceInfo[]>('list_hardware_inputs');
    } catch (e) {
        hardwareInputs = [];
        hardwareInputsLoadError = String(e);
    } finally {
        hardwareInputsLoaded = true;
    }
    await tick();
    nameInputEl?.focus();
});

async function commit() {
    submitAttempted = true;
    if (physError) { return; }
    if (nameError) { nameInputEl?.focus(); return; }
    submitError = '';
    busy = true;
    try {
        await onCreate(kind, effectiveName, kind === 'physical_input' ? physSource : '');
        onClose();
    } catch (e) {
        submitError = String(e);
    } finally {
        busy = false;
    }
}

function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); commit(); }
}

function selectKind(k: InputKind) {
    kind = k;
    submitAttempted = false;
}
</script>

<Modal label="Add input" maxWidth="440px" {onClose}>
    <div class="flex items-center justify-between px-4 py-3 border-b border-base-content/10">
        <h2 class="text-lg font-semibold m-0">Add Input</h2>
        <button class="btn btn-ghost btn-square btn-sm" onclick={onClose} aria-label="Close" data-modal-close>
            <Icon name="close" size={14} />
        </button>
    </div>

    <section class="px-4 py-3 border-b border-base-content/10 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0">Type</h3>
        <div class="flex flex-col gap-1.5" role="group" aria-label="Input type">
            <button
                class="grid grid-cols-[auto_1fr] grid-rows-[auto_auto] items-center gap-x-3 px-3 py-2.5 rounded-md border text-left cursor-pointer transition-colors disabled:opacity-55 disabled:cursor-not-allowed
                       {kind === 'input' ? 'bg-primary/15 border-primary [&_svg]:text-primary' : 'bg-base-100 border-base-content/15 hover:bg-base-content/5 hover:border-base-content/25 [&_svg]:text-base-content/55'}"
                onclick={() => selectKind('input')}
                disabled={busy}
                aria-pressed={kind === 'input'}
            >
                <span class="row-span-2 flex items-center"><Icon name="wave" size={14} /></span>
                <span class="text-base font-semibold">Virtual Mic</span>
                <span class="text-sm text-base-content/55 leading-snug">For Discord, OBS — captures channel monitors.</span>
            </button>
            <button
                class="grid grid-cols-[auto_1fr] grid-rows-[auto_auto] items-center gap-x-3 px-3 py-2.5 rounded-md border text-left cursor-pointer transition-colors disabled:opacity-55 disabled:cursor-not-allowed
                       {kind === 'physical_input' ? 'bg-primary/15 border-primary [&_svg]:text-primary' : 'bg-base-100 border-base-content/15 hover:bg-base-content/5 hover:border-base-content/25 [&_svg]:text-base-content/55'}"
                onclick={() => selectKind('physical_input')}
                disabled={busy}
                aria-pressed={kind === 'physical_input'}
            >
                <span class="row-span-2 flex items-center"><Icon name="mic" size={14} /></span>
                <span class="text-base font-semibold">Hardware Mic</span>
                <span class="text-sm text-base-content/55 leading-snug">A real input device. Volume controls the source.</span>
            </button>
        </div>
    </section>

    {#if kind === 'physical_input'}
        <section class="px-4 py-3 border-b border-base-content/10 flex flex-col gap-2">
            <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0 flex items-center gap-1">
                <span>Hardware input</span>
                <span class="text-error" aria-hidden="true">*</span>
            </h3>
            <select
                bind:value={physSource}
                onblur={() => physTouched = true}
                disabled={busy || hardwareInputs.length === 0}
                class="select select-bordered w-full {showPhysError ? 'select-error' : ''}"
                aria-invalid={showPhysError}
                aria-describedby={showPhysError ? 'input-phys-err' : undefined}
            >
                <option value="">— Select —</option>
                {#each hardwareInputs as s (s.name)}
                    <option value={s.name}>{s.description}</option>
                {/each}
            </select>
            {#if showPhysError}
                <p class="text-error text-sm flex items-center gap-1.5 m-0" id="input-phys-err" role="alert">
                    <Icon name="alert" size={12} />
                    <span>{physError}</span>
                </p>
            {:else if hardwareInputsLoadError}
                <p class="text-sm text-error m-0">Couldn't load hardware inputs: {hardwareInputsLoadError.slice(0, 100)}</p>
            {:else if hardwareInputsLoaded && hardwareInputs.length === 0}
                <p class="text-sm text-base-content/55 m-0 leading-snug">No hardware inputs detected. Plug in a mic or check that your audio interface is connected.</p>
            {/if}
        </section>

        {#if physSource}
            {#each inputOverlays as overlay (overlay.plugin_id + ':' + overlay.surface_id)}
                <section class="px-4 py-3 border-b border-base-content/10 flex flex-col gap-2">
                    <InputOverlay {overlay} sourceName={physSource} emit={(e) => emitOverlay(overlay.plugin_id, e)} />
                </section>
            {/each}
        {/if}
    {/if}

    <section class="px-4 py-3 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0 flex items-center gap-1.5">
            <span>{kind === 'physical_input' ? 'Display name' : 'Name'}</span>
            {#if kind === 'physical_input'}
                <span class="text-xs text-base-content/45 font-normal normal-case tracking-normal">(optional)</span>
            {:else}
                <span class="text-error" aria-hidden="true">*</span>
            {/if}
        </h3>
        <input
            bind:this={nameInputEl}
            bind:value={name}
            placeholder={kind === 'physical_input' ? 'Defaults to device name' : 'e.g. Discord Mic'}
            onkeydown={onKey}
            onblur={() => nameTouched = true}
            disabled={busy}
            class="input input-bordered w-full {showNameError ? 'input-error' : ''}"
            aria-invalid={showNameError}
            aria-describedby={showNameError ? 'input-name-err' : undefined}
        />
        {#if showNameError}
            <p class="text-error text-sm flex items-center gap-1.5 m-0" id="input-name-err" role="alert">
                <Icon name="alert" size={12} />
                <span>{nameError}</span>
            </p>
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
                <span>Add Input</span>
            {/if}
        </button>
    </div>
</Modal>
