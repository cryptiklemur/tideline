<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onMount, onDestroy } from 'svelte';
import Icon from './Icon.svelte';
import ChannelIcon from './ChannelIcon.svelte';
import ChannelOverlay from './plugin-ui/ChannelOverlay.svelte';
import type { ChannelOverlayContribution, UiEvent } from './plugin-ui/types';
import type { ChannelKind, ChannelVolumes, Mix, SinkInput } from './types';

interface Props {
    name: string;
    channelUuid?: string;
    kind: ChannelKind;
    icon?: string;
    mixes: Mix[];
    mixEnabled: Record<string, boolean>;
    meterSource: string;
    sourceName?: string;
    programs?: string[];
    appIcons?: Record<string, string | null>;
    initialVolumes?: ChannelVolumes;
    onSettings?: () => void;
    channelCardOverlays?: ChannelOverlayContribution[];
    onOverlayEmit?: (pluginId: string, event: UiEvent) => void;
    onOpenRack?: (pluginId: string, channelUuid: string) => void;
    draggable?: boolean;
    isDragging?: boolean;
    isDragOver?: boolean;
    onDragStart?: (e: DragEvent) => void;
    onDragEnd?: (e: DragEvent) => void;
    onDragEnter?: (e: DragEvent) => void;
    onDragOver?: (e: DragEvent) => void;
    onDrop?: (e: DragEvent) => void;
}

let {
    name, channelUuid = '', kind, icon = '', mixes, mixEnabled, meterSource, sourceName,
    programs = [], appIcons = {},
    initialVolumes,
    onSettings,
    channelCardOverlays = [],
    onOverlayEmit,
    onOpenRack,
    draggable = false,
    isDragging = false,
    isDragOver = false,
    onDragStart, onDragEnd, onDragEnter, onDragOver, onDrop,
}: Props = $props();

const isInput = $derived(kind === 'input' || kind === 'physical_input');

function slugName(s: string): string {
    return s.toLowerCase().replace(/[^a-z0-9]/g, '_').replace(/_+/g, '_').replace(/^_|_$/g, '');
}

let chSlug = $derived(slugName(name));

function playbackNode(mixId: string, sinkIdx: number): string {
    return `playback.${chSlug}-${mixId}-${sinkIdx}`;
}

let mixVols = $state<Record<string, number>>({});
let mixIndexes = $state<Record<string, number[]>>({});
let mixMutes = $state<Record<string, boolean>>({});

let masterMuted = $state(false);
let masterVol = $state(100);
let volumesSeeded = false;

$effect(() => {
    if (volumesSeeded || !initialVolumes) return;
    masterVol = initialVolumes.master ?? 100;
    mixVols = { ...mixVols, ...(initialVolumes.mixes ?? {}) };
    volumesSeeded = true;
});

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
let muteUnlisten: UnlistenFn | null = null;
let volUnlisten: UnlistenFn | null = null;
let sinkInputMuteUnlisten: UnlistenFn | null = null;
let rafId: number | null = null;

onMount(async () => {
    if (isInput) {
        await refreshInput();
        await refresh();
    } else {
        await refresh();
    }
    if (isInput && sourceName) {
        muteUnlisten = await listen<{ source_name: string; muted: boolean }>(
            'tideline:source_mute_changed',
            e => { if (e.payload.source_name === sourceName) masterMuted = e.payload.muted; },
        );
        volUnlisten = await listen<{ source_name: string; volume_pct: number }>(
            'tideline:source_volume_changed',
            e => { if (e.payload.source_name === sourceName) masterVol = e.payload.volume_pct; },
        );
    }
    sinkInputMuteUnlisten = await listen<{ index: number; muted: boolean }>(
        'tideline:sink_input_mute_changed',
        e => {
            for (const mixId of Object.keys(mixIndexes)) {
                if (mixIndexes[mixId].includes(e.payload.index)) {
                    mixMutes = { ...mixMutes, [mixId]: e.payload.muted };
                }
            }
        },
    );
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
    muteUnlisten?.();
    volUnlisten?.();
    sinkInputMuteUnlisten?.();
    if (rafId !== null) cancelAnimationFrame(rafId);
});

async function refresh() {
    const inputs = await invoke<SinkInput[]>('get_sink_inputs');
    const byNode = new Map(inputs.map(i => [i.node_name, i]));
    const nextIdx: Record<string, number[]> = {};
    const nextMutes: Record<string, boolean> = {};
    let anyMuted = false;
    let anyUnmuted = false;
    for (const m of mixes) {
        nextIdx[m.id] = [];
        let mixMuted = false;
        let mixHasInput = false;
        for (let i = 0; i < m.sinks.length; i++) {
            const node = playbackNode(m.id, i);
            const inp = byNode.get(node);
            if (inp) {
                nextIdx[m.id].push(inp.index);
                if (inp.muted) mixMuted = true;
                mixHasInput = true;
                if (inp.muted) anyMuted = true;
                else anyUnmuted = true;
            }
        }
        nextMutes[m.id] = mixHasInput && mixMuted;
    }
    mixIndexes = nextIdx;
    mixMutes = nextMutes;
    if (!isInput) {
        masterMuted = anyMuted && !anyUnmuted;
    }
}

async function refreshInput() {
    if (!sourceName) return;
    try {
        const state = await invoke<[number, boolean] | null>('get_source_state', { name: sourceName });
        if (state) {
            masterVol = state[0];
            masterMuted = state[1];
        }
    } catch {}
}

async function setMixVolume(mixId: string, v: number) {
    const prev = mixVols;
    mixVols = { ...mixVols, [mixId]: v };
    try {
        await invoke('set_channel_mix_volume', { channel: name, mixId, pct: v });
    } catch {
        mixVols = prev;
    }
}

async function toggleMixMute(mixId: string) {
    const prev = mixMutes;
    const next = !mixMutes[mixId];
    mixMutes = { ...mixMutes, [mixId]: next };
    const idxs = mixIndexes[mixId] ?? [];
    try {
        await Promise.all(idxs.map(idx => invoke('set_mute', { index: idx, muted: next })));
    } catch {
        mixMutes = prev;
    }
}

async function toggleMasterMute() {
    const prevMuted = masterMuted;
    const prevMixMutes = mixMutes;
    masterMuted = !masterMuted;
    try {
        if (isInput) {
            if (sourceName) await invoke('set_source_mute', { name: sourceName, muted: masterMuted });
            return;
        }
        const calls: Promise<unknown>[] = [];
        const next: Record<string, boolean> = {};
        for (const m of mixes) {
            const idxs = mixIndexes[m.id] ?? [];
            next[m.id] = masterMuted;
            for (const idx of idxs) {
                calls.push(invoke('set_mute', { index: idx, muted: masterMuted }));
            }
        }
        mixMutes = next;
        await Promise.all(calls);
    } catch {
        masterMuted = prevMuted;
        mixMutes = prevMixMutes;
    }
}

async function setMasterVolume(v: number) {
    const prevVol = masterVol;
    masterVol = v;
    try {
        if (isInput) {
            if (sourceName) await invoke('set_source_volume', { name: sourceName, pct: v });
            return;
        }
        await invoke('set_channel_master_volume', { channel: name, pct: v });
    } catch {
        masterVol = prevVol;
    }
}

let levelPct = $derived(Math.max(0, Math.min(1, displayLevel)));
let peakPct = $derived(Math.max(0, Math.min(1, peakHold)));

function bindCellSlider(node: HTMLDivElement, onChange: (v: number) => void) {
    let pendingX: number | null = null;
    let raf = 0;

    function valueFromX(clientX: number): number {
        const rect = node.getBoundingClientRect();
        const ratio = (clientX - rect.left) / rect.width;
        return Math.max(0, Math.min(100, Math.round(ratio * 100)));
    }
    function flush() {
        raf = 0;
        if (pendingX === null) return;
        const v = valueFromX(pendingX);
        pendingX = null;
        onChange(v);
    }
    function move(e: PointerEvent) {
        pendingX = e.clientX;
        if (raf === 0) raf = requestAnimationFrame(flush);
    }
    function up() {
        if (raf !== 0) { cancelAnimationFrame(raf); raf = 0; }
        if (pendingX !== null) { onChange(valueFromX(pendingX)); pendingX = null; }
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        window.removeEventListener('pointercancel', up);
    }
    function down(e: PointerEvent) {
        if (e.button !== 0) return;
        e.preventDefault();
        e.stopPropagation();
        try { node.setPointerCapture(e.pointerId); } catch {}
        onChange(valueFromX(e.clientX));
        window.addEventListener('pointermove', move);
        window.addEventListener('pointerup', up);
        window.addEventListener('pointercancel', up);
    }
    function wheel(e: WheelEvent) {
        e.preventDefault();
        const delta = e.deltaY > 0 ? -2 : 2;
        const current = parseFloat(node.dataset.value ?? '0');
        const next = Math.max(0, Math.min(100, current + delta));
        onChange(next);
    }
    function dbl() { onChange(100); }
    node.addEventListener('pointerdown', down);
    node.addEventListener('wheel', wheel, { passive: false });
    node.addEventListener('dblclick', dbl);
    return {
        destroy() {
            node.removeEventListener('pointerdown', down);
            node.removeEventListener('wheel', wheel);
            node.removeEventListener('dblclick', dbl);
        },
    };
}

const METER_SEGMENTS = 14;
const SEGMENT_INDICES = Array.from({ length: METER_SEGMENTS }, (_, i) => i);
function segmentColor(i: number): string {
    const ratio = (i + 1) / METER_SEGMENTS;
    if (ratio > 0.95) return 'var(--color-error)';
    if (ratio > 0.85) return 'var(--color-warning)';
    return 'var(--color-success)';
}
</script>

<div
    class="group/row relative grid items-stretch gap-2.5 px-6 py-1 bg-transparent border border-transparent rounded transition-all duration-150
           {masterMuted ? 'opacity-60' : ''}
           {isDragging ? 'opacity-40' : ''}"
    data-row
    role="group"
    aria-label={name}
    ondragenter={onDragEnter}
    ondragover={onDragOver}
    ondrop={onDrop}
    style:grid-template-columns="380px repeat({mixes.length}, 240px)"
>
    {#if isDragOver}
        <div class="absolute left-0 right-0 -top-0.5 h-0.5 bg-primary rounded-[1px] shadow-[0_0_8px_var(--color-primary)] pointer-events-none" aria-hidden="true"></div>
    {/if}

    <div
        class="group/cell grid grid-cols-[64px_1fr_auto] gap-2 items-center px-2 py-2 min-h-[80px] border rounded min-w-0 transition-colors
               {masterMuted ? 'bg-error/15 border-error/40' : 'bg-base-300 border-transparent hover:border-base-content/30'}"
        role="group"
        aria-label="{name} channel"
    >
        <button
            type="button"
            class="group/btn btn btn-square btn-lg relative text-base-content/55 hover:text-base-content"
            onclick={onSettings}
            draggable="false"
            aria-label="{name} settings"
            title="Channel settings"
        >
            <span
                class="absolute inset-0 flex items-center justify-center transition-all duration-150 group-hover/btn:opacity-0 group-hover/btn:scale-75 {kind === 'physical_input' ? 'text-error' : kind === 'input' ? 'text-primary' : ''}"
                aria-hidden="true"
            >
                <ChannelIcon {icon} {kind} {programs} {appIcons} size={32} />
            </span>
            <span
                class="absolute inset-0 flex items-center justify-center text-primary opacity-0 -rotate-30 transition-all duration-150 group-hover/btn:opacity-100 group-hover/btn:rotate-0"
                aria-hidden="true"
            >
                <Icon name="settings" size={26} />
            </span>
        </button>

        <span class="text-sm font-medium text-base-content leading-tight break-words line-clamp-2 min-w-0" title={name}>{name}</span>

        <div class="flex items-center gap-2 flex-shrink-0">
            {#each channelCardOverlays as o (o.plugin_id + ':' + o.surface_id)}
                <div
                    class="flex items-center"
                    role="button"
                    tabindex="-1"
                    onclickcapture={(e) => {
                        e.stopPropagation();
                        if (channelUuid) onOpenRack?.(o.plugin_id, channelUuid);
                    }}
                >
                    <ChannelOverlay overlay={o} emit={(ev) => onOverlayEmit?.(o.plugin_id, ev)} />
                </div>
            {/each}
            <button
                type="button"
                class="w-6 h-6 flex-shrink-0 flex items-center justify-center bg-transparent border-none rounded p-0 cursor-pointer transition-colors hover:text-base-content
                       {masterMuted ? 'text-error' : 'text-base-content/55'}"
                onclick={toggleMasterMute}
                draggable="false"
                aria-label="{name} mute"
                aria-pressed={masterMuted}
                title={masterMuted ? 'Unmute' : 'Mute'}
            >
                <Icon name={masterMuted ? 'volume-mute' : 'volume'} size={14} />
            </button>
            <div
                class="group/slider relative w-[120px] h-[36px] cursor-grab active:cursor-grabbing outline-none touch-none flex items-center"
                role="slider"
                aria-label="{name} master volume"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={masterVol}
                aria-valuetext="{masterVol}%"
                aria-disabled={masterMuted}
                tabindex={masterMuted ? -1 : 0}
                data-value={masterVol}
                draggable="false"
                use:bindCellSlider={(v) => setMasterVolume(v)}
            >
                <div class="absolute inset-x-0 top-1/2 -translate-y-1/2 h-[8px] flex gap-[2px] pointer-events-none" aria-hidden="true">
                    {#each SEGMENT_INDICES as i}
                        {@const threshold = (i + 1) / METER_SEGMENTS}
                        {@const inVolume = masterVol / 100 >= threshold}
                        {@const lit = !masterMuted && inVolume && levelPct >= threshold}
                        <div
                            class="flex-1 rounded-[1px] transition-opacity duration-75"
                            style:background={masterMuted ? 'var(--color-base-content)' : segmentColor(i)}
                            style:opacity={masterMuted ? (inVolume ? 0.35 : 0.1) : lit ? 1 : inVolume ? 0.55 : 0.12}
                        ></div>
                    {/each}
                </div>
                <div
                    class="absolute top-1/2 -translate-y-1/2 -ml-[3px] w-[6px] h-[18px] rounded-[2px] border border-base-100 shadow-[var(--shadow-thumb-flat)] pointer-events-none transition-[background-color] {masterMuted ? 'bg-base-content/45' : 'bg-primary'}"
                    style:left="{masterVol}%"
                    aria-hidden="true"
                ></div>
            </div>
        </div>
    </div>

    {#each mixes as mix (mix.id)}
        {@const enabled = mixEnabled[mix.id] !== false}
        {@const empty = mix.sinks.length === 0}
        {@const muted = mixMutes[mix.id] || masterMuted}
        {@const vol = mixVols[mix.id] ?? 100}
        <div
            class="relative flex items-center gap-3 px-4 py-3 min-h-[80px] border rounded min-w-0 transition-colors
                   {muted ? 'bg-error/15 border-error/40' : empty ? 'bg-transparent border-base-content/15 border-dashed' : 'bg-base-300 border-transparent hover:border-base-content/30'}
                   {empty ? 'opacity-40' : !enabled ? 'opacity-60' : ''}"
        >
            <button
                class="w-6 h-6 flex-shrink-0 flex items-center justify-center bg-transparent border-none rounded p-0 cursor-pointer transition-colors disabled:cursor-not-allowed disabled:opacity-50
                       {muted ? 'text-error' : 'text-base-content/55 hover:text-base-content hover:bg-base-300'}"
                onclick={() => toggleMixMute(mix.id)}
                disabled={empty || !enabled}
                aria-label="Mute {name} on {mix.name}"
                aria-pressed={muted}
                title={muted ? 'Unmute' : 'Mute'}
            >
                {#if muted}
                    <Icon name="volume-mute" size={14} />
                {:else}
                    <Icon name="volume" size={14} />
                {/if}
            </button>
            <div
                class="group/slider relative flex-1 min-w-0 h-[36px] cursor-grab active:cursor-grabbing outline-none touch-none flex items-center {empty ? 'invisible' : ''}"
                role="slider"
                aria-label="{name} {mix.name} volume"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={vol}
                aria-valuetext="{vol}%"
                aria-disabled={muted}
                tabindex={muted ? -1 : 0}
                data-value={vol}
                use:bindCellSlider={(v) => setMixVolume(mix.id, v)}
            >
                <div class="absolute inset-x-0 top-1/2 -translate-y-1/2 h-[8px] flex gap-[2px] pointer-events-none" aria-hidden="true">
                    {#each SEGMENT_INDICES as i}
                        {@const threshold = (i + 1) / METER_SEGMENTS}
                        {@const inVolume = vol / 100 >= threshold}
                        {@const lit = !muted && enabled && !empty && inVolume && levelPct >= threshold}
                        <div
                            class="flex-1 rounded-[1px] transition-opacity duration-75"
                            style:background={muted ? 'var(--color-base-content)' : segmentColor(i)}
                            style:opacity={muted ? (inVolume ? 0.35 : 0.1) : lit ? 1 : inVolume ? 0.55 : 0.12}
                        ></div>
                    {/each}
                </div>
                <div
                    class="absolute top-1/2 -translate-y-1/2 -ml-[3px] w-[6px] h-[18px] rounded-[2px] border border-base-100 shadow-[var(--shadow-thumb-flat)] pointer-events-none transition-[background-color] {muted ? 'bg-base-content/45' : 'bg-primary'}"
                    style:left="{vol}%"
                    aria-hidden="true"
                ></div>
            </div>
        </div>
    {/each}
</div>
