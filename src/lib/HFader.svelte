<script lang="ts">
interface Props {
    label: string;
    value: number;
    disabled?: boolean;
    valueLabel?: string;
    valueText?: string;
    onchange: (v: number) => void;
}

let { label, value, disabled = false, valueLabel, valueText, onchange }: Props = $props();

let trackEl = $state<HTMLDivElement>();
let pendingX: number | null = null;
let rafId = 0;

function valueFromX(clientX: number): number {
    if (!trackEl) return value;
    const rect = trackEl.getBoundingClientRect();
    const ratio = (clientX - rect.left) / rect.width;
    return Math.max(0, Math.min(100, Math.round(ratio * 100)));
}

function flushPending() {
    rafId = 0;
    if (pendingX === null) return;
    const next = valueFromX(pendingX);
    pendingX = null;
    if (next !== value) onchange(next);
}

function onWindowPointerMove(e: PointerEvent) {
    pendingX = e.clientX;
    if (rafId === 0) rafId = requestAnimationFrame(flushPending);
}

function onWindowPointerUp() {
    if (rafId !== 0) {
        cancelAnimationFrame(rafId);
        rafId = 0;
    }
    if (pendingX !== null) {
        const next = valueFromX(pendingX);
        pendingX = null;
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
    const next = valueFromX(e.clientX);
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
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') next = Math.min(100, value + 2);
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') next = Math.max(0, value - 2);
    else if (e.key === 'PageUp') next = Math.min(100, value + 10);
    else if (e.key === 'PageDown') next = Math.max(0, value - 10);
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = 100;
    else return;
    e.preventDefault();
    if (next !== value) onchange(next);
}
</script>

<div class="flex items-center gap-2.5 flex-1 min-w-0 transition-opacity {disabled ? 'opacity-30' : ''}">
    <div
        class="group relative flex-1 h-[36px] min-w-0 outline-none touch-none flex items-center {disabled ? 'cursor-not-allowed' : 'cursor-grab active:cursor-grabbing'}"
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
        aria-valuetext={valueText ?? `${value}%`}
        aria-disabled={disabled}
    >
        <div class="absolute left-0 top-1/2 -translate-y-1/2 w-full h-1.5 rounded-full bg-base-content/20 overflow-hidden pointer-events-none">
            <div class="absolute inset-0 bg-primary origin-left will-change-transform" style:transform="scaleX({value / 100})"></div>
        </div>
        <div
            class="absolute top-1/2 -translate-y-1/2 -ml-[3px] w-[6px] h-[18px] rounded-[2px] border border-base-100 shadow-[var(--shadow-thumb-flat)] pointer-events-none transition-[background-color] {disabled ? 'bg-base-content/45' : 'bg-primary'}"
            style:left="{value}%"
        ></div>
    </div>
    <span class="mono text-sm min-w-[56px] text-right tabular-nums whitespace-nowrap {disabled ? 'text-base-content/55' : 'text-base-content'}">{valueLabel ?? value}</span>
</div>
