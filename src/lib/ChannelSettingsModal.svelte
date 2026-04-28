<script lang="ts">
import { onMount, tick } from 'svelte';
import Icon from './Icon.svelte';
import ChannelIcon, { CHANNEL_ICON_OPTIONS, defaultIconForKind } from './ChannelIcon.svelte';
import Modal from './Modal.svelte';
import ProgramPicker from './ProgramPicker.svelte';
import type { ChannelKind } from './types';

interface Props {
    name: string;
    kind: ChannelKind;
    icon: string;
    programs: string[];
    appIcons?: Record<string, string | null>;
    existingNames: string[];
    onRename: (newName: string) => Promise<void>;
    onChangeIcon: (icon: string) => Promise<void> | void;
    onAddProgram: (binary: string) => Promise<void> | void;
    onRemoveProgram: (binary: string) => Promise<void> | void;
    onDelete: () => Promise<void> | void;
    onClose: () => void;
}

let {
    name, kind, icon, programs, appIcons = {}, existingNames,
    onRename, onChangeIcon, onAddProgram, onRemoveProgram, onDelete, onClose,
}: Props = $props();

let fileInputEl = $state<HTMLInputElement>();
let iconError = $state('');

async function pickLucide(value: string) {
    iconError = '';
    await onChangeIcon(value);
}

async function resetIcon() {
    iconError = '';
    await onChangeIcon('');
}

async function uploadIcon() {
    fileInputEl?.click();
}

async function onFilePicked(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) {
        iconError = 'Please select an image file';
        return;
    }
    if (file.size > 256 * 1024) {
        iconError = 'Image too large (max 256KB)';
        return;
    }
    try {
        const dataUrl = await readAsDataUrl(file);
        const resized = await resizeDataUrl(dataUrl, 64);
        iconError = '';
        await onChangeIcon(resized);
    } catch (err) {
        iconError = String(err);
    }
}

function readAsDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error ?? new Error('read failed'));
        reader.readAsDataURL(file);
    });
}

function resizeDataUrl(src: string, target: number): Promise<string> {
    return new Promise((resolve, reject) => {
        const img = new window.Image();
        img.onload = () => {
            const canvas = document.createElement('canvas');
            canvas.width = target;
            canvas.height = target;
            const ctx = canvas.getContext('2d');
            if (!ctx) { reject(new Error('canvas unavailable')); return; }
            const ratio = Math.min(img.width, img.height);
            const sx = (img.width - ratio) / 2;
            const sy = (img.height - ratio) / 2;
            ctx.imageSmoothingQuality = 'high';
            ctx.drawImage(img, sx, sy, ratio, ratio, 0, 0, target, target);
            resolve(canvas.toDataURL('image/png'));
        };
        img.onerror = () => reject(new Error('image decode failed'));
        img.src = src;
    });
}

const isCustom = $derived(icon.startsWith('data:'));
const isAuto = $derived(!isCustom && !icon);
const activeLucide = $derived(!isCustom && icon ? icon : defaultIconForKind(kind));

const isInput = $derived(kind === 'input' || kind === 'physical_input');

let nameDraft = $state('');
let lastSeenName = $state('');
let nameInputEl = $state<HTMLInputElement>();
let renaming = $state(false);
let renameError = $state('');
let pickerOpen = $state(false);
let pendingDelete = $state(false);
let deleteTimer: ReturnType<typeof setTimeout> | null = null;

$effect(() => {
    if (name !== lastSeenName) {
        nameDraft = name;
        lastSeenName = name;
    }
});

onMount(async () => {
    await tick();
    nameInputEl?.focus();
    nameInputEl?.select();
});

async function commitRename() {
    const next = nameDraft.trim();
    if (!next || next === name) { renameError = ''; return; }
    if (existingNames.filter(n => n !== name).includes(next)) {
        renameError = 'Name is already taken';
        return;
    }
    renameError = '';
    renaming = true;
    try {
        await onRename(next);
    } catch (e) {
        renameError = String(e);
    } finally {
        renaming = false;
    }
}

function onNameKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); commitRename(); }
    else if (e.key === 'Escape') { e.preventDefault(); nameDraft = name; }
}

function pickProgram(binary: string) {
    pickerOpen = false;
    onAddProgram(binary);
}

function requestDelete() {
    if (pendingDelete) {
        if (deleteTimer) { clearTimeout(deleteTimer); deleteTimer = null; }
        pendingDelete = false;
        Promise.resolve(onDelete()).then(() => onClose());
        return;
    }
    pendingDelete = true;
    if (deleteTimer) clearTimeout(deleteTimer);
    deleteTimer = setTimeout(() => { pendingDelete = false; deleteTimer = null; }, 3000);
}
</script>

<Modal label="Channel settings" maxWidth="480px" {onClose}>
    <div class="flex items-center gap-2.5 px-4 py-3 border-b border-base-content/10">
        <div class="w-7 h-7 rounded-md flex items-center justify-center bg-base-100 text-base-content/55" aria-hidden="true">
            <ChannelIcon {icon} {kind} {programs} {appIcons} size={18} />
        </div>
        <h2 class="flex-1 m-0 text-lg font-semibold truncate min-w-0">{name}</h2>
        <button class="btn btn-ghost btn-square btn-sm" onclick={onClose} aria-label="Close" data-modal-close>
            <Icon name="close" size={14} />
        </button>
    </div>

    <section class="px-4 py-3 flex flex-col gap-2 border-b border-base-content/10">
        <h3 class="text-xs font-bold uppercase tracking-widest text-base-content/55 m-0">Name</h3>
        <input
            bind:this={nameInputEl}
            bind:value={nameDraft}
            disabled={renaming}
            onkeydown={onNameKey}
            onblur={commitRename}
            placeholder="Channel name"
            class="input input-bordered w-full"
        />
        {#if renameError}<p class="text-sm text-error m-0">{renameError}</p>{/if}
    </section>

    <section class="px-4 py-3 flex flex-col gap-2 border-b border-base-content/10">
        <h3 class="text-xs font-bold uppercase tracking-widest text-base-content/55 m-0">Icon</h3>
        {#if !isInput}
            <button
                type="button"
                class="flex items-center gap-3 px-3 py-2 rounded-md border cursor-pointer transition-colors text-left
                       {isAuto ? 'bg-primary/15 border-primary text-primary' : 'bg-base-100 border-base-content/15 text-base-content/55 hover:bg-base-content/5 hover:border-base-content/25 hover:text-base-content'}"
                onclick={resetIcon}
                aria-pressed={isAuto}
            >
                <span class="w-9 h-9 flex items-center justify-center rounded bg-base-100/80 flex-shrink-0">
                    <ChannelIcon icon="" {kind} {programs} {appIcons} size={28} />
                </span>
                <span class="flex flex-col min-w-0">
                    <span class="text-sm font-semibold">Auto</span>
                    <span class="text-xs text-base-content/55">
                        {#if programs.length === 0}No apps yet — falls back to {defaultIconForKind(kind)}
                        {:else}Uses up to 4 app icons from this channel{/if}
                    </span>
                </span>
            </button>
        {/if}
        <div class="grid grid-cols-8 gap-1" role="radiogroup" aria-label="Channel icon">
            {#each CHANNEL_ICON_OPTIONS as opt (opt)}
                {@const active = !isCustom && !isAuto && activeLucide === opt}
                <button
                    type="button"
                    class="aspect-square flex items-center justify-center p-0 rounded-md border cursor-pointer transition-colors
                           {active ? 'bg-primary/15 border-primary text-primary' : 'bg-base-100 border-base-content/15 text-base-content/55 hover:bg-base-content/5 hover:border-base-content/25 hover:text-base-content'}"
                    onclick={() => pickLucide(opt)}
                    aria-label="Use {opt} icon"
                    aria-pressed={active}
                >
                    <Icon name={opt} size={16} />
                </button>
            {/each}
            {#if isCustom}
                <div class="aspect-square flex items-center justify-center bg-primary/15 border border-primary text-primary rounded-md cursor-default" aria-label="Custom uploaded icon">
                    <ChannelIcon {icon} {kind} size={20} />
                </div>
            {/if}
        </div>
        <div class="flex gap-2 mt-1.5 flex-wrap">
            <button type="button" class="inline-flex items-center gap-1 px-2.5 py-1 bg-transparent border border-dashed border-base-content/20 rounded-md text-base-content/55 text-sm font-semibold cursor-pointer transition-colors hover:bg-primary/10 hover:border-primary hover:text-primary" onclick={uploadIcon}>
                <Icon name="image" size={12} />
                <span>Upload custom…</span>
            </button>
            {#if icon}
                <button type="button" class="inline-flex items-center gap-1 px-2.5 py-1 bg-transparent border border-dashed border-base-content/20 rounded-md text-base-content/55 text-sm font-semibold cursor-pointer transition-colors hover:bg-error/15 hover:border-error hover:text-error" onclick={resetIcon}>
                    <Icon name="close" size={12} />
                    <span>Reset</span>
                </button>
            {/if}
        </div>
        <input
            type="file"
            accept="image/png,image/jpeg,image/webp,image/svg+xml"
            bind:this={fileInputEl}
            onchange={onFilePicked}
            hidden
        />
        {#if iconError}<p class="text-sm text-error m-0">{iconError}</p>{/if}
    </section>

    {#if !isInput}
        <section class="px-4 py-3 flex flex-col gap-2 border-b border-base-content/10">
            <h3 class="text-xs font-bold uppercase tracking-widest text-base-content/55 m-0">Apps</h3>
            <p class="text-sm text-base-content/55 m-0 leading-snug">Programs that send their audio to this channel.</p>
            <div class="flex flex-col gap-1 mt-1">
                {#each programs as prog (prog)}
                    <div class="flex items-center gap-2 pl-2.5 pr-1.5 py-1.5 bg-base-100 border border-base-content/15 rounded-md">
                        <span class="flex-1 font-mono text-sm truncate min-w-0">{prog}</span>
                        <button class="btn btn-ghost btn-square btn-xs hover:bg-error hover:text-error-content" onclick={() => onRemoveProgram(prog)} title="Remove" aria-label="Remove {prog}">
                            <Icon name="close" size={11} />
                        </button>
                    </div>
                {:else}
                    <p class="m-0 py-2 text-sm text-base-content/55 italic">No apps assigned yet.</p>
                {/each}
            </div>
            <button class="self-start inline-flex items-center gap-1.5 px-3 py-1.5 bg-transparent border border-dashed border-base-content/20 rounded-md text-base-content/55 text-sm font-semibold cursor-pointer transition-colors hover:bg-primary/10 hover:border-primary hover:text-primary" onclick={() => pickerOpen = true}>
                <Icon name="plus" size={12} />
                <span>Add app</span>
            </button>
        </section>
    {/if}

    <section class="px-4 py-3 flex flex-col gap-2">
        <h3 class="text-xs font-bold uppercase tracking-widest text-error m-0">Delete</h3>
        <p class="text-sm text-base-content/55 m-0 leading-snug">
            {#if isInput}Removing this input also removes any references to it from output channels.{:else}Removing this channel also disconnects any apps routed through it.{/if}
        </p>
        <button
            class="self-start btn btn-sm {pendingDelete ? 'btn-error' : 'btn-error btn-outline'}"
            onclick={requestDelete}
        >
            <Icon name="close" size={11} />
            <span>{pendingDelete ? 'Click again to confirm' : 'Delete channel'}</span>
        </button>
    </section>
</Modal>

{#if pickerOpen}
    <ProgramPicker
        existing={programs}
        onPick={pickProgram}
        onClose={() => pickerOpen = false}
    />
{/if}
