<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onMount, onDestroy } from 'svelte';
import HFader from './HFader.svelte';
import Icon from './Icon.svelte';
import SourcePicker from './SourcePicker.svelte';
import { isControlBlacklisted } from './cardControlBlacklist';
import type { CardControl, ChannelKind } from './types';
import { pluginUi } from './plugin-ui/pluginUi.svelte';
import InputOverlay from './plugin-ui/InputOverlay.svelte';
import type { UiEvent } from './plugin-ui/types';

interface Props {
    channelUuid?: string;
    name: string;
    kind: ChannelKind;
    sinkName: string;
    physicalSource: string;
    meterSource: string;
    sources?: string[];
    onAddSource?: (src: string) => void;
    onRemoveSource?: (src: string) => void;
    onDelete?: () => void;
    onHide?: () => void;
}

let {
    channelUuid, name, kind, sinkName, physicalSource, meterSource,
    sources = [],
    onAddSource, onRemoveSource, onDelete, onHide,
}: Props = $props();

const EFFECTS_PLUGIN_ID = 'tideline-effects';

let inVol = $state(100);
let softGainAmp = $state(1.0);
let muted = $state(false);
let sourcePickerOpen = $state(false);
let pendingDelete = $state(false);
let deleteTimer: ReturnType<typeof setTimeout> | null = null;

let cardId = $state<number | null>(null);
let cardControls = $state<CardControl[]>([]);
let cardLoaded = $state(false);
let muteUnlisten: UnlistenFn | null = null;
let volUnlisten: UnlistenFn | null = null;
let cardCtrlVolUnlisten: UnlistenFn | null = null;
let cardCtrlMuteUnlisten: UnlistenFn | null = null;

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

async function pullSoftGain() {
    if (!channelUuid) return;
    try {
        const s = await invoke<{ lowcut: boolean; clipguard: boolean; input_gain: number }>('tideline_plugin_request', {
            pluginId: EFFECTS_PLUGIN_ID,
            method: 'effects.get_dsp_state',
            params: { channel_uuid: channelUuid },
        });
        if (typeof s.input_gain === 'number') {
            softGainAmp = s.input_gain;
            inVol = Math.round(softGainAmp * 100);
        }
    } catch (e) {
        console.warn('get_dsp_state failed', e);
    }
}

onMount(async () => {
    await refresh();
    if (kind === 'physical_input' && physicalSource) {
        cardId = await invoke<number | null>('get_card_for_source', { source: physicalSource });
        if (cardId !== null) {
            await reloadCardControls();
        }
        cardLoaded = true;
        await pullSoftGain();
        muteUnlisten = await listen<{ source_name: string; muted: boolean }>(
            'tideline:source_mute_changed',
            e => { if (e.payload.source_name === physicalSource) muted = e.payload.muted; },
        );
        volUnlisten = await listen<{ source_name: string; volume_pct: number }>(
            'tideline:source_volume_changed',
            e => {
                if (e.payload.source_name === physicalSource && !primaryCapture) {
                    inVol = e.payload.volume_pct;
                }
            },
        );
        cardCtrlVolUnlisten = await listen<{ card: number; name: string; volume_pct: number }>(
            'tideline:card_control_volume_changed',
            e => {
                if (cardId === null || e.payload.card !== cardId) return;
                cardControls = cardControls.map(c =>
                    c.name === e.payload.name ? { ...c, volume_percent: e.payload.volume_pct } : c
                );
                // Note: deliberately does not mirror amixer changes onto inVol
                // — the slider is now the software gain stage on the effects
                // plugin, not the alsa preamp.
            },
        );
        cardCtrlMuteUnlisten = await listen<{ card: number; name: string; muted: boolean }>(
            'tideline:card_control_mute_changed',
            e => {
                if (cardId === null || e.payload.card !== cardId) return;
                cardControls = cardControls.map(c =>
                    c.name === e.payload.name ? { ...c, muted: e.payload.muted } : c
                );
            },
        );
    } else if (kind === 'input' && sinkName) {
        muteUnlisten = await listen<{ sink_name: string; muted: boolean }>(
            'tideline:sink_mute_changed',
            e => { if (e.payload.sink_name === sinkName) muted = e.payload.muted; },
        );
        volUnlisten = await listen<{ sink_name: string; volume_pct: number }>(
            'tideline:sink_volume_changed',
            e => { if (e.payload.sink_name === sinkName) inVol = e.payload.volume_pct; },
        );
    }
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
    muteUnlisten?.();
    volUnlisten?.();
    cardCtrlVolUnlisten?.();
    cardCtrlMuteUnlisten?.();
});

let captureControls = $derived(cardControls.filter(c => c.is_capture));

let inputOverlays = $derived(
    kind === 'physical_input' && physicalSource
        ? pluginUi.contributions.input_overlays.filter(o =>
            o.input_filter.kind === 'all'
            || o.input_filter.kind === 'physical_only'
            || (o.input_filter.kind === 'source_names' && o.input_filter.names.includes(physicalSource))
        )
        : []
);

function emitOverlay(pluginId: string, ev: UiEvent) {
    pluginUi.emit(pluginId, ev);
}
let visibleCaptureControls = $derived(captureControls.filter(c => !isControlBlacklisted(physicalSource, c.name)));
let primaryCapture = $derived(
    kind === 'physical_input'
        ? captureControls.find(c => c.has_volume) ?? null
        : null
);

let softGainDb = $derived(softGainAmp > 0 ? 20 * Math.log10(softGainAmp) : null);
let gainDb = $derived(kind === 'physical_input' ? softGainDb : (primaryCapture?.current_db ?? null));

async function reloadCardControls() {
    if (cardId === null) return;
    const next = await invoke<CardControl[]>('list_card_controls', { card: cardId });
    cardControls = next;
    // inVol is driven by software gain, not the alsa preamp — see onVolChange.
}

async function setControlVolume(controlName: string, pct: number) {
    if (cardId === null) return;
    const prev = cardControls;
    cardControls = cardControls.map(c => c.name === controlName ? { ...c, volume_percent: pct } : c);
    try {
        await invoke('set_card_control_volume', { card: cardId, name: controlName, pct });
        await reloadCardControls();
    } catch {
        cardControls = prev;
    }
}

async function toggleControlMute(controlName: string) {
    if (cardId === null) return;
    const ctl = cardControls.find(c => c.name === controlName);
    if (!ctl) return;
    const prev = cardControls;
    const next = !ctl.muted;
    cardControls = cardControls.map(c => c.name === controlName ? { ...c, muted: next } : c);
    try {
        await invoke('set_card_control_mute', { card: cardId, name: controlName, muted: next });
    } catch {
        cardControls = prev;
    }
}

async function refresh() {
    if (kind === 'physical_input') {
        // Mute state still comes from the pulse source. inVol is owned by
        // the software gain stage (pulled separately in pullSoftGain).
        const state = await invoke<[number, boolean] | null>('get_source_state', { name: physicalSource });
        if (state) { muted = state[1]; }
    } else {
        const state = await invoke<[number, boolean] | null>('get_sink_state', { name: sinkName });
        if (state) { inVol = state[0]; muted = state[1]; }
    }
}

async function onVolChange(v: number) {
    const prevVol = inVol;
    const prevAmp = softGainAmp;
    inVol = v;
    try {
        if (kind === 'physical_input') {
            // Route through the effects-plugin software gain stage. Hardware
            // preamps (e.g. Wave XLR firmware DSP) ignore host volume changes
            // in some bands, so the software stage is the only reliable trim.
            if (!channelUuid) {
                throw new Error('channel uuid required for software gain');
            }
            const amp = v / 100;
            softGainAmp = amp;
            await invoke('tideline_plugin_request', {
                pluginId: EFFECTS_PLUGIN_ID,
                method: 'effects.set_input_gain',
                params: { channel_uuid: channelUuid, amp },
            });
        } else {
            await invoke('set_sink_volume', { name: sinkName, pct: v });
        }
    } catch {
        inVol = prevVol;
        softGainAmp = prevAmp;
    }
}

function formatDb(db: number): string {
    const sign = db > 0 ? '+' : '';
    return `${sign}${db.toFixed(1)} dB`;
}

async function toggleMute() {
    const prev = muted;
    muted = !muted;
    try {
        if (kind === 'physical_input') {
            await invoke('set_source_mute', { name: physicalSource, muted });
        } else {
            await invoke('set_sink_mute', { name: sinkName, muted });
        }
    } catch {
        muted = prev;
    }
}

function pickSource(src: string) {
    sourcePickerOpen = false;
    onAddSource?.(src);
}

function shortSourceName(s: string): string {
    if (s.endsWith('.monitor')) {
        const base = s.slice(0, -8);
        return base.replace(/^alsa_output\.usb-/, '').replace(/^sink\./, '').slice(0, 32) + ' (mon)';
    }
    return s.replace(/^alsa_input\.usb-/, '').slice(0, 36);
}

function requestDelete() {
    if (!onDelete) return;
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
</script>

<div class="flex flex-col gap-3 max-w-[620px] transition-opacity {muted ? 'opacity-85' : ''}">
    <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">
            {kind === 'physical_input' ? 'Input gain' : 'Output volume'}
        </span>
        <HFader
            label="{name} volume"
            value={inVol}
            disabled={muted}
            valueLabel={gainDb !== null ? formatDb(gainDb) : `${inVol}%`}
            valueText={gainDb !== null ? formatDb(gainDb) : undefined}
            onchange={onVolChange}
        />
    </section>

    <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
        <div class="flex items-baseline justify-between gap-2">
            <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Level</span>
        </div>
        <div class="h-2.5 rounded border border-base-content/15 overflow-hidden relative bg-[linear-gradient(90deg,var(--color-success)_0%,var(--color-success)_70%,var(--color-warning)_75%,var(--color-warning)_90%,var(--color-error)_95%,var(--color-error)_100%)]" aria-hidden="true">
            <div class="absolute inset-0 bg-base-100 origin-right will-change-transform" style:transform="scaleX({Math.max(0, Math.min(1, 1 - displayLevel))})"></div>
            {#if peakHold > 0.01}
                <div class="absolute top-0 bottom-0 w-0.5 bg-base-content pointer-events-none" style:left="{Math.min(99.5, Math.max(0, peakHold * 100 - 0.5))}%"></div>
            {/if}
        </div>
        <div class="flex justify-between font-mono text-xs text-base-content/55 px-0.5" aria-hidden="true">
            <span>-60</span>
            <span>-40</span>
            <span>-20</span>
            <span>-10</span>
            <span>0 dB</span>
        </div>
    </section>

    {#if kind === 'input'}
        <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
            <div class="flex items-baseline justify-between gap-2">
                <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Sources</span>
                <span class="text-[10px] text-base-content/55 leading-snug">Audio captured by this virtual mic.</span>
            </div>
            <div class="flex flex-wrap gap-1.5" role="group" aria-label="{name} sources">
                {#each sources as src (src)}
                    <button
                        class="inline-flex items-center gap-1.5 bg-base-200 border border-base-content/15 rounded-md text-base-content/70 font-mono text-[10px] px-2 py-1 cursor-pointer transition-colors max-w-full min-w-0 hover:bg-error hover:text-error-content hover:border-error"
                        onclick={() => onRemoveSource?.(src)}
                        title={src}
                        aria-label="Remove {shortSourceName(src)}"
                    >
                        <span class="overflow-hidden text-ellipsis whitespace-nowrap min-w-0">{shortSourceName(src)}</span>
                        <Icon name="close" size={9} />
                    </button>
                {/each}
                <button class="inline-flex items-center gap-1.5 bg-transparent border border-dashed border-base-content/15 rounded-md text-base-content/55 font-bold text-[10px] tracking-widest px-2 py-1 cursor-pointer transition-colors hover:bg-primary/10 hover:text-primary hover:border-primary" onclick={() => sourcePickerOpen = true} title="Add source">
                    <Icon name="plus" size={11} />
                    <span>SOURCE</span>
                </button>
            </div>
        </section>
    {:else if kind === 'physical_input'}
        <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
            <div class="flex items-baseline justify-between gap-2">
                <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Hardware source</span>
            </div>
            <p class="m-0 font-mono text-sm text-base-content/70 overflow-hidden text-ellipsis whitespace-nowrap" title={physicalSource}>{physicalSource}</p>
        </section>

        {#if cardLoaded && cardId !== null && visibleCaptureControls.length > 0}
            <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
                <div class="flex items-baseline justify-between gap-2">
                    <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Device controls</span>
                    <span class="text-[10px] text-base-content/55 leading-snug">Card #{cardId}</span>
                </div>
                <ul class="list-none m-0 p-0 flex flex-col gap-3">
                    {#each visibleCaptureControls as ctl (ctl.name)}
                        {#if ctl.has_volume || ctl.has_switch}
                            <li class="flex flex-col gap-2">
                                <div class="flex items-center gap-2 min-w-0">
                                    <span class="text-sm text-base-content overflow-hidden text-ellipsis whitespace-nowrap min-w-0 flex-1" title={ctl.name}>{ctl.name}</span>
                                    {#if ctl.has_switch}
                                        <button
                                            class="flex-shrink-0 px-2.5 py-0.5 border rounded-md font-mono text-xs font-bold tracking-wider cursor-pointer transition-colors
                                                   {ctl.muted ? 'bg-error border-error text-error-content' : 'bg-base-200 border-base-content/15 text-base-content/70 hover:bg-base-content/5 hover:text-base-content'}"
                                            aria-label="{ctl.name} mute"
                                            aria-pressed={ctl.muted}
                                            onclick={() => toggleControlMute(ctl.name)}
                                        >{ctl.muted ? 'OFF' : 'ON'}</button>
                                    {/if}
                                </div>
                                {#if ctl.has_volume}
                                    <HFader
                                        label="{ctl.name} volume"
                                        value={ctl.volume_percent}
                                        valueLabel={ctl.current_db !== null ? formatDb(ctl.current_db) : `${ctl.volume_percent}%`}
                                        disabled={ctl.muted}
                                        onchange={(v) => setControlVolume(ctl.name, v)}
                                    />
                                {/if}
                            </li>
                        {/if}
                    {/each}
                </ul>
            </section>
        {:else if cardLoaded && cardId === null}
            <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
                <div class="flex items-baseline justify-between gap-2">
                    <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Device controls</span>
                </div>
                <p class="m-0 text-[10px] text-base-content/55 leading-snug">No ALSA card found for this source. Hardware-level controls are unavailable.</p>
            </section>
        {/if}

        {#each inputOverlays as overlay (overlay.plugin_id + ':' + overlay.surface_id)}
            <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
                <InputOverlay {overlay} sourceName={physicalSource} emit={(e) => emitOverlay(overlay.plugin_id, e)} />
            </section>
        {/each}
    {/if}

    <div class="flex gap-2">
        <button
            class="flex-1 py-2.5 border rounded-md text-[10px] font-bold tracking-widest cursor-pointer transition-colors
                   {muted ? 'bg-error border-error text-error-content' : 'bg-base-200 border-base-content/15 text-base-content/70 hover:bg-base-content/5 hover:text-base-content hover:border-base-content/25'}"
            aria-label="{name} mute"
            aria-pressed={muted}
            onclick={toggleMute}
        >{muted ? 'MUTED' : 'MUTE'}</button>

        {#if onHide}
            <button
                class="flex items-center justify-center gap-1.5 px-4 py-2.5 border rounded-md text-[10px] font-bold tracking-wider uppercase cursor-pointer transition-colors bg-transparent border-base-content/15 text-base-content/55 hover:bg-warning hover:text-warning-content hover:border-warning"
                onclick={onHide}
                title="Hide this input from the UI and tray (unhide in Settings)"
            >
                <Icon name="close" size={11} />
                Hide input
            </button>
        {/if}

        {#if onDelete}
            <button
                class="flex items-center justify-center gap-1.5 px-4 py-2.5 border rounded-md text-[10px] font-bold tracking-wider uppercase cursor-pointer transition-colors
                       {pendingDelete ? 'bg-error border-error text-error-content' : 'bg-transparent border-base-content/15 text-base-content/55 hover:bg-error hover:text-error-content hover:border-error'}"
                onclick={requestDelete}
                title={pendingDelete ? 'Click again to confirm' : 'Delete this input'}
            >
                <Icon name="close" size={11} />
                {pendingDelete ? 'Confirm delete' : 'Delete input'}
            </button>
        {/if}
    </div>
</div>

{#if sourcePickerOpen}
    <SourcePicker
        existing={sources}
        excludeSink={sinkName}
        onPick={pickSource}
        onClose={() => sourcePickerOpen = false}
    />
{/if}
