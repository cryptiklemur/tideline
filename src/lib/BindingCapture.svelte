<script lang="ts">
import Icon from './Icon.svelte';
import type { Binding, Modifier } from './types';

interface Props {
    label: string;
    sublabel?: string;
    binding: Binding | null;
    onSet: (binding: Binding) => Promise<void> | void;
    onClear: () => Promise<void> | void;
}
let { label, sublabel, binding, onSet, onClear }: Props = $props();

let capturing = $state(false);
let captureError = $state<string | null>(null);
let captureEl: HTMLButtonElement | undefined = $state();

const KEY_CODE_MAP: Record<string, string> = {
    Space: 'Space', Enter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', Delete: 'Delete',
    Insert: 'Insert', Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown',
    ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
    Escape: 'Escape', CapsLock: 'CapsLock', NumLock: 'NumLock', ScrollLock: 'ScrollLock',
    PrintScreen: 'PrintScreen', Pause: 'Pause',
    Minus: 'Minus', Equal: 'Equal',
    BracketLeft: 'BracketLeft', BracketRight: 'BracketRight',
    Semicolon: 'Semicolon', Quote: 'Quote',
    Comma: 'Comma', Period: 'Period', Slash: 'Slash', Backslash: 'Backslash', Backquote: 'Backquote',
};

function fmtKey(code: string, key: string): string | null {
    if (code.startsWith('Key')) return code.slice(3);
    if (code.startsWith('Digit')) return code.slice(5);
    if (/^F\d{1,2}$/.test(code)) return code;
    if (KEY_CODE_MAP[code]) return KEY_CODE_MAP[code];
    if (key && key.length === 1) return key.toUpperCase();
    return null;
}

function modsFromEvent(e: KeyboardEvent | MouseEvent): Modifier[] {
    const m: Modifier[] = [];
    if (e.ctrlKey) m.push('ctrl');
    if (e.altKey) m.push('alt');
    if (e.shiftKey) m.push('shift');
    if (e.metaKey) m.push('super');
    return m;
}

function formatBinding(b: Binding): string {
    const order: Record<Modifier, number> = { ctrl: 0, alt: 1, shift: 2, super: 3 };
    const labels: Record<Modifier, string> = { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', super: 'Super' };
    const mods = [...b.mods].sort((a, b) => order[a] - order[b]).map(m => labels[m]);
    const tail = b.kind === 'keyboard' ? b.key : b.button;
    return mods.length ? `${mods.join('+')}+${tail}` : tail;
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
    const key = fmtKey(e.code, e.key);
    if (!key) { captureError = 'unsupported key'; return; }
    const b: Binding = { kind: 'keyboard', mods: modsFromEvent(e), key };
    try { await onSet(b); stopCapture(); } catch (err) { captureError = String(err); }
}

const MOUSE_BUTTONS: Record<number, string> = {
    0: 'Mouse1', 1: 'Mouse3', 2: 'Mouse2',
    3: 'Mouse4', 4: 'Mouse5',
};

async function onCaptureMouse(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const button = MOUSE_BUTTONS[e.button];
    if (!button) { captureError = `unsupported mouse button (${e.button})`; return; }
    const b: Binding = { kind: 'mouse', mods: modsFromEvent(e), button };
    try { await onSet(b); stopCapture(); } catch (err) { captureError = String(err); }
}

async function clearBinding() {
    try { await onClear(); } catch (err) { captureError = String(err); }
}
</script>

<div class="flex items-center gap-3 px-2.5 py-1.5 rounded hover:bg-base-300/50 min-w-0">
    <div class="flex flex-col min-w-0 flex-1">
        <span class="text-sm text-base-content overflow-hidden text-ellipsis whitespace-nowrap">{label}</span>
        {#if sublabel}
            <span class="text-[11px] text-base-content/55 leading-snug">{sublabel}</span>
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
            onmousedown={onCaptureMouse}
            oncontextmenu={(e) => e.preventDefault()}
            onblur={stopCapture}
        >
            press a key or mouse button… (esc cancel)
        </button>
    {:else if binding}
        <span class="flex items-center gap-1 flex-shrink-0">
            <kbd class="kbd kbd-sm font-mono text-xs">{formatBinding(binding)}</kbd>
            <button type="button" class="w-6 h-6 flex items-center justify-center rounded bg-transparent border-none cursor-pointer text-base-content/55 hover:bg-base-content/10 hover:text-error transition-colors" onclick={clearBinding} aria-label="Clear binding" title="Clear">
                <Icon name="close" size={10} />
            </button>
            <button type="button" class="px-2 py-0.5 rounded border border-base-content/15 bg-transparent text-base-content/55 text-[10px] font-bold tracking-widest uppercase cursor-pointer hover:bg-base-300 hover:text-base-content transition-colors" onclick={startCapture}>Rebind</button>
        </span>
    {:else}
        <button type="button" class="px-2 py-0.5 rounded border border-dashed border-base-content/15 bg-transparent text-base-content/55 text-[10px] font-bold tracking-widest uppercase cursor-pointer hover:border-primary hover:text-primary hover:bg-primary/10 transition-colors flex-shrink-0" onclick={startCapture}>Bind</button>
    {/if}
</div>
