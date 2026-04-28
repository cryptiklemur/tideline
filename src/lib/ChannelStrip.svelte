<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onMount, onDestroy } from 'svelte';
import Fader from './Fader.svelte';
import Icon from './Icon.svelte';
import ProgramPicker from './ProgramPicker.svelte';
import SourcePicker from './SourcePicker.svelte';
import type { SinkInput, OutputMode, ChannelKind } from './types';

interface Props {
    name: string;
    kind: ChannelKind;
    hpNode: string;
    spNode: string;
    sinkName: string;
    physicalSource?: string;
    meterSource?: string;
    mode: OutputMode;
    programs?: string[];
    sources?: string[];
    onDelete?: () => void;
    onAddProgram?: (binary: string) => void;
    onRemoveProgram?: (binary: string) => void;
    onAddSource?: (src: string) => void;
    onRemoveSource?: (src: string) => void;
}

let {
    name, kind, hpNode, spNode, sinkName, physicalSource = '', meterSource = '', mode,
    programs = [], sources = [],
    onDelete, onAddProgram, onRemoveProgram, onAddSource, onRemoveSource
}: Props = $props();

let hpVol = $state(100);
let spVol = $state(100);
let inVol = $state(100);
let hpIndex = $state<number | null>(null);
let spIndex = $state<number | null>(null);

let muted = $state(false);
let linked = $state(true);
let pendingDelete = $state(false);
let deleteTimer: ReturnType<typeof setTimeout> | null = null;
let programPickerOpen = $state(false);
let sourcePickerOpen = $state(false);
let menuOpen = $state(false);

function kindIcon(k: ChannelKind): 'speaker' | 'mic' | 'wave' {
    if (k === 'physical_input') return 'mic';
    if (k === 'input') return 'wave';
    return 'speaker';
}

$effect(() => {
    if (!menuOpen) return;
    function onDocMouseDown(e: MouseEvent) {
        const target = e.target as Element | null;
        if (!target) return;
        if (!target.closest('.strip-menu, .strip-menu-btn')) {
            menuOpen = false;
        }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
});

let hpMuted = $derived(muted || mode === 'speakers');
let spMuted = $derived(muted || mode === 'headphones');
let hpActive = $derived(!hpMuted);
let spActive = $derived(!spMuted);

const METER_FLOOR_DB = -60;
const PEAK_HOLD_MS = 1200;
const PEAK_FALL_PER_SEC = 0.4;
const RMS_FALL_PER_SEC = 1.6;

function ampToBar(amp: number): number {
    if (amp <= 0) return 0;
    const db = 20 * Math.log10(amp);
    if (db <= METER_FLOOR_DB) return 0;
    if (db >= 0) return 1;
    return (db - METER_FLOOR_DB) / -METER_FLOOR_DB;
}

let displayLevel = $state(0);
let peakHold = $state(0);
let lastPeakAt = 0;
let levelUnlisten: UnlistenFn | null = null;
let rafId: number | null = null;

onMount(async () => {
    await refresh();
    if (meterSource) {
        const eventName = `level:${meterSource.replace(/\./g, '_')}`;
        levelUnlisten = await listen<{ peak: number; rms: number }>(eventName, e => {
            const rmsBar = ampToBar(e.payload.rms);
            const peakBar = ampToBar(e.payload.peak);
            if (rmsBar > displayLevel) displayLevel = rmsBar;
            if (peakBar >= peakHold) {
                peakHold = peakBar;
                lastPeakAt = performance.now();
            }
        });
    }
    let prev = performance.now();
    const tick = (now: number) => {
        const dt = Math.min(0.1, (now - prev) / 1000);
        prev = now;
        if (displayLevel > 0) displayLevel = Math.max(0, displayLevel - RMS_FALL_PER_SEC * dt);
        if (now - lastPeakAt > PEAK_HOLD_MS) {
            peakHold = Math.max(displayLevel, peakHold - PEAK_FALL_PER_SEC * dt);
        }
        rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
});

onDestroy(() => {
    if (levelUnlisten) levelUnlisten();
    if (rafId !== null) cancelAnimationFrame(rafId);
});

async function refresh() {
    if (kind === 'output') {
        const inputs = await invoke<SinkInput[]>('get_sink_inputs');
        for (const inp of inputs) {
            if (inp.node_name === hpNode) { hpIndex = inp.index; hpVol = inp.volume; }
            if (inp.node_name === spNode) { spIndex = inp.index; spVol = inp.volume; }
        }
    } else if (kind === 'physical_input') {
        const state = await invoke<[number, boolean] | null>('get_source_state', { name: physicalSource });
        if (state) { inVol = state[0]; muted = state[1]; }
    } else {
        const state = await invoke<[number, boolean] | null>('get_sink_state', { name: sinkName });
        if (state) { inVol = state[0]; muted = state[1]; }
    }
}

async function onHpChange(v: number) {
    hpVol = v;
    if (hpIndex !== null) await invoke('set_volume', { index: hpIndex, pct: v });
    if (linked) {
        spVol = v;
        if (spIndex !== null) await invoke('set_volume', { index: spIndex, pct: v });
    }
}

async function onSpChange(v: number) {
    spVol = v;
    if (spIndex !== null) await invoke('set_volume', { index: spIndex, pct: v });
    if (linked) {
        hpVol = v;
        if (hpIndex !== null) await invoke('set_volume', { index: hpIndex, pct: v });
    }
}

async function onInChange(v: number) {
    inVol = v;
    if (kind === 'physical_input') {
        await invoke('set_source_volume', { name: physicalSource, pct: v });
    } else {
        await invoke('set_sink_volume', { name: sinkName, pct: v });
    }
}

async function toggleMute() {
    muted = !muted;
    if (kind === 'output') {
        if (hpIndex !== null) await invoke('set_mute', { index: hpIndex, muted: muted || mode === 'speakers' });
        if (spIndex !== null) await invoke('set_mute', { index: spIndex, muted: muted || mode === 'headphones' });
    } else if (kind === 'physical_input') {
        await invoke('set_source_mute', { name: physicalSource, muted });
    } else {
        await invoke('set_sink_mute', { name: sinkName, muted });
    }
}

function onMenuButtonClick() {
    if (!onDelete) return;
    if (pendingDelete) {
        if (deleteTimer) { clearTimeout(deleteTimer); deleteTimer = null; }
        pendingDelete = false;
        onDelete();
        return;
    }
    menuOpen = !menuOpen;
}

function requestDelete() {
    pendingDelete = true;
    menuOpen = false;
    if (deleteTimer) clearTimeout(deleteTimer);
    deleteTimer = setTimeout(() => { pendingDelete = false; deleteTimer = null; }, 3000);
}

function pickProgram(binary: string) { programPickerOpen = false; onAddProgram?.(binary); }
function pickSource(src: string) { sourcePickerOpen = false; onAddSource?.(src); }

function shortSourceName(s: string): string {
    if (s.endsWith('.monitor')) {
        const base = s.slice(0, -8);
        return base.replace(/^alsa_output\.usb-/, '').replace(/^sink\./, '').slice(0, 18) + ' (mon)';
    }
    return s.replace(/^alsa_input\.usb-/, '').slice(0, 22);
}

const stripBorder = $derived.by(() => {
    if (kind === 'input') return 'border-primary';
    if (kind === 'physical_input') return 'border-error';
    return 'border-base-content/15 hover:border-base-content/25';
});
const stripAccent = $derived.by(() => {
    if (kind === 'input') return 'shadow-[inset_3px_0_0_var(--color-primary)]';
    if (kind === 'physical_input') return 'shadow-[inset_3px_0_0_var(--color-error)]';
    return '';
});
const stripIconBg = $derived.by(() => {
    if (kind === 'input') return 'bg-primary/15 text-primary';
    if (kind === 'physical_input') return 'bg-error/15 text-error';
    return 'bg-base-200 text-base-content';
});
</script>

<div
    class="flex flex-col items-stretch gap-2 px-2.5 pt-3 pb-2.5 bg-base-100 border rounded-md flex-[1_1_140px] min-w-[130px] max-w-[140px] transition-colors relative {stripBorder} {stripAccent} {muted ? 'opacity-85' : ''}"
    role="group"
    aria-label={name}
>
    <div class="flex items-center gap-1.5 relative min-h-7">
        <div class="w-7 h-7 rounded-md flex items-center justify-center flex-shrink-0 {stripIconBg}" aria-hidden="true">
            <Icon name={kindIcon(kind)} size={14} />
        </div>
        <div class="flex-1 min-w-0 text-base font-medium text-base-content overflow-hidden text-ellipsis whitespace-nowrap text-center leading-tight">{name}</div>
        {#if onDelete}
            <button
                class="h-6 min-w-6 px-1.5 border rounded-md flex items-center justify-center flex-shrink-0 cursor-pointer transition-all
                       {pendingDelete
                         ? 'bg-error border-error text-error-content'
                         : (menuOpen ? 'bg-base-content/5 border-base-content/15 text-base-content' : 'bg-transparent border-transparent text-base-content/55 hover:bg-base-content/5 hover:border-base-content/15 hover:text-base-content')}"
                onclick={onMenuButtonClick}
                aria-label={pendingDelete ? 'Confirm delete' : 'Channel menu'}
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                title={pendingDelete ? 'Click again to confirm' : 'Channel menu'}
            >
                {#if pendingDelete}
                    <span class="text-[10px] font-bold tracking-wider">DELETE?</span>
                {:else}
                    <span class="inline-flex transition-transform {menuOpen ? 'rotate-180' : ''}"><Icon name="chevron-down" size={11} /></span>
                {/if}
            </button>
            {#if menuOpen}
                <div class="strip-menu absolute top-[calc(100%+4px)] right-0 min-w-[140px] p-1 bg-base-100 border border-base-content/25 rounded-md shadow-xl z-10 flex flex-col gap-0.5" role="menu">
                    <button class="flex items-center gap-2 px-2 py-1.5 border-none bg-transparent text-base-content text-sm text-left rounded-md cursor-pointer transition-colors hover:bg-error hover:text-error-content" role="menuitem" onclick={requestDelete}>
                        <Icon name="close" size={11} />
                        Delete channel
                    </button>
                </div>
            {/if}
        {/if}
    </div>

    {#if kind === 'output'}
        <div class="flex flex-wrap gap-1 pb-1.5 border-b border-dashed border-base-content/15" role="group" aria-label="{name} programs">
            {#each programs as prog (prog)}
                <button class="inline-flex items-center gap-1 bg-base-200 border border-base-content/15 rounded-md text-base-content/70 font-mono text-[10px] px-1.5 py-0.5 cursor-pointer transition-colors max-w-full min-w-0 hover:bg-error hover:text-error-content hover:border-error" onclick={() => onRemoveProgram?.(prog)} title="Click to remove" aria-label="Remove {prog}">
                    <span class="overflow-hidden text-ellipsis whitespace-nowrap min-w-0 flex-[0_1_auto]">{prog}</span>
                    <Icon name="close" size={9} />
                </button>
            {/each}
            <button class="inline-flex items-center gap-1 bg-transparent border border-dashed border-base-content/15 rounded-md text-base-content/55 font-bold text-[10px] tracking-wider px-1.5 py-0.5 cursor-pointer transition-colors hover:bg-primary/10 hover:text-primary hover:border-primary" onclick={() => programPickerOpen = true} title="Add program">
                <Icon name="plus" size={11} /><span>APP</span>
            </button>
        </div>
    {:else if kind === 'input'}
        <div class="flex flex-wrap gap-1 pb-1.5 border-b border-dashed border-base-content/15" role="group" aria-label="{name} sources">
            {#each sources as src (src)}
                <button class="inline-flex items-center gap-1 bg-base-200 border border-base-content/15 rounded-md text-base-content/70 font-mono text-[10px] px-1.5 py-0.5 cursor-pointer transition-colors max-w-full min-w-0 hover:bg-error hover:text-error-content hover:border-error" onclick={() => onRemoveSource?.(src)} title="Click to remove" aria-label="Remove {shortSourceName(src)}">
                    <span class="overflow-hidden text-ellipsis whitespace-nowrap min-w-0 flex-[0_1_auto]">{shortSourceName(src)}</span>
                    <Icon name="close" size={9} />
                </button>
            {/each}
            <button class="inline-flex items-center gap-1 bg-transparent border border-dashed border-base-content/15 rounded-md text-base-content/55 font-bold text-[10px] tracking-wider px-1.5 py-0.5 cursor-pointer transition-colors hover:bg-primary/10 hover:text-primary hover:border-primary" onclick={() => sourcePickerOpen = true} title="Add source">
                <Icon name="plus" size={11} /><span>SOURCE</span>
            </button>
        </div>
    {/if}

    {#if kind === 'output'}
        <div class="flex gap-2 items-center relative flex-1 min-h-0">
            {#if linked}
                <div class="absolute top-[22px] bottom-[22px] left-1/2 -translate-x-1/2 w-[18px] border-x border-primary opacity-45 pointer-events-none rounded-[1px]" aria-hidden="true"></div>
            {/if}
            <Fader icon="headphones" label="{name} headphone volume" value={hpVol} disabled={!hpActive} onchange={onHpChange} />
            <button
                class="self-center w-6 h-6 p-0 border rounded-md flex items-center justify-center flex-shrink-0 cursor-pointer transition-colors z-[1]
                       {linked ? 'bg-primary border-primary text-primary-content' : 'bg-base-200 border-base-content/15 text-base-content/70 hover:bg-base-content/5 hover:border-base-content/25 hover:text-base-content'}"
                onclick={() => linked = !linked}
                aria-label="{name} link faders"
                aria-pressed={linked}
                title={linked ? 'Unlink faders' : 'Link faders'}
            >
                <Icon name={linked ? 'link' : 'unlink'} size={11} />
            </button>
            <Fader icon="speaker" label="{name} speaker volume" value={spVol} disabled={!spActive} onchange={onSpChange} />
        </div>
    {:else}
        <div class="flex justify-center flex-1 min-h-0">
            <Fader icon="wave" label="{name} volume" value={inVol} disabled={muted} onchange={onInChange} />
        </div>
    {/if}

    {#if kind === 'physical_input'}
        <p class="m-0 pt-1 border-t border-dashed border-base-content/15 font-mono text-[10px] text-base-content/55 overflow-hidden text-ellipsis whitespace-nowrap text-center" title={physicalSource}>{physicalSource}</p>
    {/if}

    <div class="h-1.5 rounded-sm border border-base-content/15 overflow-hidden relative bg-[linear-gradient(90deg,var(--color-success)_0%,var(--color-success)_70%,var(--color-warning)_75%,var(--color-warning)_90%,var(--color-error)_95%,var(--color-error)_100%)]" aria-hidden="true">
        <div class="absolute inset-0 bg-base-100 origin-right will-change-transform" style:transform="scaleX({Math.max(0, Math.min(1, 1 - displayLevel))})"></div>
        {#if peakHold > 0.01}
            <div class="absolute top-0 bottom-0 w-0.5 bg-base-content pointer-events-none" style:left="{Math.min(99.5, Math.max(0, peakHold * 100 - 0.5))}%"></div>
        {/if}
    </div>

    <button
        class="w-full py-1.5 border rounded-md text-[10px] font-bold tracking-widest cursor-pointer transition-colors
               {muted ? 'bg-error border-error text-error-content' : 'bg-base-200 border-base-content/15 text-base-content/70 hover:bg-base-content/5 hover:border-base-content/25 hover:text-base-content'}"
        aria-label="{name} mute"
        aria-pressed={muted}
        onclick={toggleMute}
    >{muted ? 'MUTED' : 'MUTE'}</button>
</div>

{#if programPickerOpen}
    <ProgramPicker
        existing={programs}
        onPick={pickProgram}
        onClose={() => programPickerOpen = false}
    />
{/if}
{#if sourcePickerOpen}
    <SourcePicker
        existing={sources}
        excludeSink={sinkName}
        onPick={pickSource}
        onClose={() => sourcePickerOpen = false}
    />
{/if}
