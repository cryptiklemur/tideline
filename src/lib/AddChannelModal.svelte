<script lang="ts">
import { onMount, tick } from 'svelte';
import Icon from './Icon.svelte';
import Modal from './Modal.svelte';

interface Props {
    existingNames: string[];
    onCreate: (name: string) => Promise<void>;
    onClose: () => void;
}

let { existingNames, onCreate, onClose }: Props = $props();

let name = $state('');
let busy = $state(false);
let submitError = $state('');
let nameTouched = $state(false);
let submitAttempted = $state(false);
let nameInputEl = $state<HTMLInputElement>();

let nameError = $derived.by(() => {
    const trimmed = name.trim();
    if (!trimmed) return 'Name is required';
    if (existingNames.includes(trimmed)) return 'A channel with that name already exists';
    return '';
});
let showNameError = $derived((nameTouched || submitAttempted) && !!nameError);
let canSubmit = $derived(!busy && !nameError);

onMount(async () => {
    await tick();
    nameInputEl?.focus();
});

async function commit() {
    submitAttempted = true;
    if (nameError) {
        nameInputEl?.focus();
        return;
    }
    submitError = '';
    busy = true;
    try {
        await onCreate(name.trim());
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
</script>

<Modal label="Add channel" maxWidth="440px" {onClose}>
    <div class="flex items-center justify-between px-4 py-3 border-b border-base-content/10">
        <h2 class="text-lg font-semibold m-0">Add Channel</h2>
        <button class="btn btn-ghost btn-square btn-sm" onclick={onClose} aria-label="Close" data-modal-close>
            <Icon name="close" size={14} />
        </button>
    </div>

    <section class="px-4 py-3 flex flex-col gap-2">
        <h3 class="text-sm font-bold uppercase tracking-widest text-base-content/55 m-0 flex items-center gap-1">
            <span>Name</span>
            <span class="text-error" aria-hidden="true">*</span>
        </h3>
        <input
            bind:this={nameInputEl}
            bind:value={name}
            placeholder="e.g. Music, Game, Discord"
            onkeydown={onKey}
            onblur={() => nameTouched = true}
            disabled={busy}
            class="input input-bordered w-full {showNameError ? 'input-error' : ''}"
            aria-invalid={showNameError}
            aria-describedby={showNameError ? 'channel-name-err' : 'channel-name-hint'}
        />
        {#if showNameError}
            <p class="text-error text-sm flex items-center gap-1.5 m-0" id="channel-name-err" role="alert">
                <Icon name="alert" size={12} />
                <span>{nameError}</span>
            </p>
        {:else}
            <p class="text-sm text-base-content/55 m-0 leading-snug" id="channel-name-hint">A channel groups apps so you can route them to your mixes independently.</p>
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
                <span>Add Channel</span>
            {/if}
        </button>
    </div>
</Modal>
