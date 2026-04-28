<script lang="ts">
import Icon from './Icon.svelte';
import type { KeybindAction } from './types';

interface Props {
    label: string;
    sublabel?: string;
    action: KeybindAction;
    bound: string | null;
    onSet: (accel: string, action: KeybindAction) => Promise<void> | void;
    onClear: (accel: string) => Promise<void> | void;
    isConflict?: (accel: string) => boolean;
}

let { label, sublabel, action, bound, onSet, onClear, isConflict }: Props = $props();

let capturing = $state(false);
let captureError = $state<string | null>(null);
let captureEl: HTMLButtonElement | undefined = $state();

function fmtKey(code: string, key: string): string | null {
    if (code.startsWith('Key')) return code.slice(3);
    if (code.startsWith('Digit')) return code.slice(5);
    if (/^F\d{1,2}$/.test(code)) return code;
    if (code === 'Space') return 'Space';
    if (code === 'Enter') return 'Enter';
    if (code === 'Tab') return 'Tab';
    if (code === 'Backspace') return 'Backspace';
    if (code === 'Delete') return 'Delete';
    if (code === 'Home') return 'Home';
    if (code === 'End') return 'End';
    if (code === 'PageUp') return 'PageUp';
    if (code === 'PageDown') return 'PageDown';
    if (code === 'ArrowUp') return 'Up';
    if (code === 'ArrowDown') return 'Down';
    if (code === 'ArrowLeft') return 'Left';
    if (code === 'ArrowRight') return 'Right';
    if (code === 'Minus') return '-';
    if (code === 'Equal') return '=';
    if (code === 'BracketLeft') return '[';
    if (code === 'BracketRight') return ']';
    if (code === 'Semicolon') return ';';
    if (code === 'Quote') return "'";
    if (code === 'Comma') return ',';
    if (code === 'Period') return '.';
    if (code === 'Slash') return '/';
    if (code === 'Backslash') return '\\';
    if (code === 'Backquote') return '`';
    if (key && key.length === 1) return key.toUpperCase();
    return null;
}

function startCapture() {
    captureError = null;
    capturing = true;
    queueMicrotask(() => captureEl?.focus());
}

function stopCapture() {
    capturing = false;
    captureError = null;
}

async function onCaptureKey(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape' && !e.ctrlKey && !e.altKey && !e.metaKey) {
        stopCapture();
        return;
    }
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return;
    const mods: string[] = [];
    if (e.ctrlKey) mods.push('Ctrl');
    if (e.altKey) mods.push('Alt');
    if (e.shiftKey) mods.push('Shift');
    if (e.metaKey) mods.push('Super');
    const key = fmtKey(e.code, e.key);
    if (!key) {
        captureError = 'unsupported key';
        return;
    }
    const accel = mods.length ? [...mods, key].join('+') : key;
    if (isConflict?.(accel)) {
        captureError = `${accel} already bound`;
        return;
    }
    try {
        if (bound) await onClear(bound);
        await onSet(accel, action);
        stopCapture();
    } catch (err) {
        captureError = String(err);
    }
}

async function clearBinding() {
    if (!bound) return;
    try {
        await onClear(bound);
    } catch (err) {
        captureError = String(err);
    }
}
</script>

<div class="flex items-center gap-3 px-2.5 py-1.5 rounded hover:bg-base-300/50 min-w-0">
    <div class="flex flex-col min-w-0 flex-1">
        <span class="text-sm text-base-content overflow-hidden text-ellipsis whitespace-nowrap">{label}</span>
        {#if sublabel}
            <span class="font-mono text-[10px] text-base-content/55 overflow-hidden text-ellipsis whitespace-nowrap">{sublabel}</span>
        {/if}
        {#if captureError}
            <span class="text-[10px] text-error">{captureError}</span>
        {/if}
    </div>

    {#if capturing}
        <button
            bind:this={captureEl}
            type="button"
            class="px-2 py-1 rounded border border-primary bg-primary/15 text-primary text-[11px] font-bold tracking-widest uppercase outline-none ring-2 ring-primary/40"
            onkeydown={onCaptureKey}
            onblur={stopCapture}
        >
            press keys… (esc cancel)
        </button>
    {:else if bound}
        <span class="flex items-center gap-1 flex-shrink-0">
            <kbd class="kbd kbd-sm font-mono text-xs">{bound}</kbd>
            <button
                type="button"
                class="w-6 h-6 flex items-center justify-center rounded bg-transparent border-none cursor-pointer text-base-content/55 hover:bg-base-content/10 hover:text-error transition-colors"
                onclick={clearBinding}
                aria-label="Clear keybind"
                title="Clear"
            >
                <Icon name="close" size={10} />
            </button>
            <button
                type="button"
                class="px-2 py-0.5 rounded border border-base-content/15 bg-transparent text-base-content/55 text-[10px] font-bold tracking-widest uppercase cursor-pointer hover:bg-base-300 hover:text-base-content transition-colors"
                onclick={startCapture}
            >Rebind</button>
        </span>
    {:else}
        <button
            type="button"
            class="px-2 py-0.5 rounded border border-dashed border-base-content/15 bg-transparent text-base-content/55 text-[10px] font-bold tracking-widest uppercase cursor-pointer hover:border-primary hover:text-primary hover:bg-primary/10 transition-colors flex-shrink-0"
            onclick={startCapture}
        >Bind</button>
    {/if}
</div>
