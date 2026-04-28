<script lang="ts">
import Icon from './Icon.svelte';

interface Props {
    icon: 'headphones' | 'speaker' | 'wave';
    label: string;
    value: number;
    disabled?: boolean;
    onchange: (v: number) => void;
}

let { icon, label, value, disabled = false, onchange }: Props = $props();

let trackEl = $state<HTMLDivElement>();
let pendingY: number | null = null;
let rafId = 0;

function valueFromY(clientY: number): number {
    if (!trackEl) return value;
    const rect = trackEl.getBoundingClientRect();
    const ratio = 1 - (clientY - rect.top) / rect.height;
    return Math.max(0, Math.min(100, Math.round(ratio * 100)));
}

function flushPending() {
    rafId = 0;
    if (pendingY === null) return;
    const next = valueFromY(pendingY);
    pendingY = null;
    if (next !== value) onchange(next);
}

function onWindowPointerMove(e: PointerEvent) {
    pendingY = e.clientY;
    if (rafId === 0) rafId = requestAnimationFrame(flushPending);
}

function onWindowPointerUp() {
    if (rafId !== 0) {
        cancelAnimationFrame(rafId);
        rafId = 0;
    }
    if (pendingY !== null) {
        const next = valueFromY(pendingY);
        pendingY = null;
        if (next !== value) onchange(next);
    }
    window.removeEventListener('pointermove', onWindowPointerMove);
    window.removeEventListener('pointerup', onWindowPointerUp);
    window.removeEventListener('pointercancel', onWindowPointerUp);
}

function handlePointerDown(e: PointerEvent) {
    if (disabled) return;
    if (e.button !== 0) return;
    e.preventDefault();
    const next = valueFromY(e.clientY);
    if (next !== value) onchange(next);
    window.addEventListener('pointermove', onWindowPointerMove);
    window.addEventListener('pointerup', onWindowPointerUp);
    window.addEventListener('pointercancel', onWindowPointerUp);
}

function handleWheel(e: WheelEvent) {
    if (disabled) return;
    e.preventDefault();
    const delta = e.deltaY > 0 ? -2 : 2;
    const next = Math.max(0, Math.min(100, value + delta));
    if (next !== value) onchange(next);
}

function handleDouble() {
    if (disabled) return;
    onchange(100);
}

function handleKey(e: KeyboardEvent) {
    if (disabled) return;
    let next = value;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') next = Math.min(100, value + 2);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') next = Math.max(0, value - 2);
    else if (e.key === 'PageUp') next = Math.min(100, value + 10);
    else if (e.key === 'PageDown') next = Math.max(0, value - 10);
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = 100;
    else return;
    e.preventDefault();
    if (next !== value) onchange(next);
}
</script>

<div class="flex flex-col items-center gap-1.5 flex-1 min-h-0 transition-opacity {disabled ? 'opacity-30' : ''}">
    <span class="h-5 flex items-center justify-center {disabled ? 'text-base-content/55' : 'text-base-content'}">
        <Icon name={icon} size={14} />
    </span>
    <div
        class="group relative w-9 h-full min-h-[120px] max-h-[480px] outline-none touch-none flex justify-center {disabled ? 'cursor-not-allowed' : 'cursor-grab active:cursor-grabbing'}"
        bind:this={trackEl}
        onpointerdown={handlePointerDown}
        onwheel={handleWheel}
        ondblclick={handleDouble}
        onkeydown={handleKey}
        role="slider"
        tabindex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={value}
        aria-valuetext="{value}%"
        aria-disabled={disabled}
    >
        <div class="absolute left-1/2 top-0 -translate-x-1/2 w-1.5 h-full rounded-full bg-base-content/20 overflow-hidden pointer-events-none">
            <div class="absolute left-0 bottom-0 w-full h-full bg-primary origin-bottom will-change-transform" style:transform="scaleY({value / 100})"></div>
        </div>
        <div
            class="absolute left-1/2 -translate-x-1/2 -mb-[9px] w-[6px] h-[18px] rounded-[2px] border border-base-100 shadow-[var(--shadow-thumb-flat)] pointer-events-none transition-[background-color] {disabled ? 'bg-base-content/45' : 'bg-primary'}"
            style:bottom="{value}%"
        ></div>
    </div>
    <span class="mono text-sm min-w-[32px] text-center {disabled ? 'text-base-content/55' : 'text-base-content'}">{value}</span>
</div>
