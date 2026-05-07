<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onMount, onDestroy } from 'svelte';
import HFader from './HFader.svelte';
import type { CardControl } from './types';

interface Props {
    sinkName: string;
    description: string;
}

let { sinkName, description }: Props = $props();

let vol = $state(100);
let muted = $state(false);
let cardId = $state<number | null>(null);
let cardControls = $state<CardControl[]>([]);
let cardLoaded = $state(false);
let lastSinkName = '';
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
let rafId: number | null = null;

let meterSource = $derived(`${sinkName}.monitor`);

async function initSink(name: string) {
    const state = await invoke<[number, boolean] | null>('get_sink_state', { name });
    if (state) { vol = state[0]; muted = state[1]; }
    cardId = await invoke<number | null>('get_card_for_sink', { sink: name });
    if (cardId !== null) {
        cardControls = await invoke<CardControl[]>('list_card_controls', { card: cardId });
    } else {
        cardControls = [];
    }
    cardLoaded = true;
}

$effect(() => {
    if (sinkName !== lastSinkName) {
        lastSinkName = sinkName;
        cardLoaded = false;
        cardId = null;
        cardControls = [];
        initSink(sinkName);
    }
});

$effect(() => {
    const eventName = `level:${meterSource.replace(/\./g, '_')}`;
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    listen<{ peak: number; rms: number }>(eventName, e => {
        const rmsBar = ampToBar(e.payload.rms);
        const peakBar = ampToBar(e.payload.peak);
        if (rmsBar > displayLevel) displayLevel = rmsBar;
        if (peakBar >= peakHold) {
            peakHold = peakBar;
            lastPeakAt = performance.now();
        }
    }).then(fn => {
        if (cancelled) { fn(); return; }
        unlisten = fn;
    });
    return () => {
        cancelled = true;
        if (unlisten) unlisten();
    };
});

onMount(async () => {
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

    muteUnlisten = await listen<{ sink_name: string; muted: boolean }>(
        'tideline:sink_mute_changed',
        e => { if (e.payload.sink_name === sinkName) muted = e.payload.muted; },
    );
    volUnlisten = await listen<{ sink_name: string; volume_pct: number }>(
        'tideline:sink_volume_changed',
        e => { if (e.payload.sink_name === sinkName) vol = e.payload.volume_pct; },
    );
    cardCtrlVolUnlisten = await listen<{ card: number; name: string; volume_pct: number }>(
        'tideline:card_control_volume_changed',
        e => {
            if (cardId === null || e.payload.card !== cardId) return;
            cardControls = cardControls.map(c =>
                c.name === e.payload.name ? { ...c, volume_percent: e.payload.volume_pct } : c
            );
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
});

onDestroy(() => {
    if (rafId !== null) cancelAnimationFrame(rafId);
    muteUnlisten?.();
    volUnlisten?.();
    cardCtrlVolUnlisten?.();
    cardCtrlMuteUnlisten?.();
});

async function onVolChange(v: number) {
    const prev = vol;
    vol = v;
    try {
        await invoke('set_sink_volume', { name: sinkName, pct: v });
    } catch {
        vol = prev;
    }
}

async function toggleMute() {
    const prev = muted;
    muted = !muted;
    try {
        await invoke('set_sink_mute', { name: sinkName, muted });
    } catch {
        muted = prev;
    }
}

async function setControlVolume(controlName: string, pct: number) {
    if (cardId === null) return;
    const prev = cardControls;
    cardControls = cardControls.map(c => c.name === controlName ? { ...c, volume_percent: pct } : c);
    try {
        await invoke('set_card_control_volume', { card: cardId, name: controlName, pct });
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
</script>

<div class="flex flex-col gap-3 max-w-[620px] transition-opacity {muted ? 'opacity-85' : ''}">
    <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Output volume</span>
        <HFader
            label="{description} volume"
            value={vol}
            valueLabel={`${vol}%`}
            disabled={muted}
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

    <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
        <div class="flex items-baseline justify-between gap-2">
            <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Device</span>
        </div>
        <p class="m-0 font-mono text-sm text-base-content/70 overflow-hidden text-ellipsis whitespace-nowrap" title={sinkName}>{sinkName}</p>
    </section>

    {#if cardLoaded && cardId !== null && cardControls.length > 0}
        <section class="flex flex-col gap-2 p-3 bg-base-100 border border-base-content/15 rounded-md">
            <div class="flex items-baseline justify-between gap-2">
                <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Device controls</span>
                <span class="text-[10px] text-base-content/55 leading-snug">Card #{cardId}</span>
            </div>
            <ul class="list-none m-0 p-0 flex flex-col gap-3">
                {#each cardControls as ctl (ctl.name)}
                    {#if ctl.has_volume || ctl.has_switch}
                        <li class="flex flex-col gap-2">
                            <div class="flex items-center gap-2 min-w-0">
                                <span class="text-sm text-base-content overflow-hidden text-ellipsis whitespace-nowrap min-w-0 flex-1" title={ctl.name}>{ctl.name}</span>
                                <div class="flex gap-1 flex-shrink-0">
                                    {#if ctl.is_capture}<span class="font-mono text-xs font-bold tracking-wider px-1.5 py-px rounded-sm border border-primary text-primary bg-base-200">CAP</span>{/if}
                                    {#if ctl.is_playback}<span class="font-mono text-xs font-bold tracking-wider px-1.5 py-px rounded-sm border border-base-content/15 text-base-content/70 bg-base-200">PLAY</span>{/if}
                                </div>
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
                                    valueLabel={ctl.current_db !== null ? `${ctl.current_db > 0 ? '+' : ''}${ctl.current_db.toFixed(1)} dB` : `${ctl.volume_percent}%`}
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
            <p class="m-0 text-[10px] text-base-content/55 leading-snug">No ALSA card found for this sink. Hardware-level controls are unavailable.</p>
        </section>
    {/if}

    <div class="flex gap-2">
        <button
            class="flex-1 py-2.5 border rounded-md text-[10px] font-bold tracking-widest cursor-pointer transition-colors
                   {muted ? 'bg-error border-error text-error-content' : 'bg-base-200 border-base-content/15 text-base-content/70 hover:bg-base-content/5 hover:text-base-content hover:border-base-content/25'}"
            aria-label="{description} mute"
            aria-pressed={muted}
            onclick={toggleMute}
        >{muted ? 'MUTED' : 'MUTE'}</button>
    </div>
</div>
