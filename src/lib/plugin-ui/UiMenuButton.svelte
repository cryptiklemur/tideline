<script lang="ts">
import { tick } from 'svelte';
import type { UiEvent, UiMenuItem, UiNode } from './types';
import UiIcon from './UiIcon.svelte';

interface Props {
    node: Extract<UiNode, { kind: 'menu_button' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();

const VIEWPORT_PAD = 8;
const MENU_GAP = 4;

let triggerEl: HTMLButtonElement | undefined = $state();
let menuEl: HTMLDivElement | undefined = $state();
let submenuRoot: HTMLDivElement | undefined = $state();

let open = $state(false);
let hoveredPath = $state<string[]>([]);
let menuPos = $state<{ top: number; left: number; minWidth: number; maxHeight: number } | null>(null);
let submenuPos = $state<{ top: number; left: number; maxHeight: number } | null>(null);
let portalTarget = $state<HTMLElement | null>(null);

function findPortalTarget(from: HTMLElement | undefined): HTMLElement {
    if (!from) return document.body;
    let el: HTMLElement | null = from;
    while (el) {
        if (el.tagName === 'DIALOG') return el;
        el = el.parentElement;
    }
    return document.body;
}

let cls = $derived(
    node.variant === 'primary' ? 'btn btn-primary btn-sm' :
    node.variant === 'warning' ? 'btn btn-warning btn-sm' :
    node.variant === 'success' ? 'btn btn-success btn-soft btn-sm' :
    node.variant === 'ghost' ? 'btn btn-ghost btn-sm' :
    'btn btn-soft btn-sm'
);

function isLeaf(item: UiMenuItem): item is Extract<UiMenuItem, { disabled?: boolean }> {
    return !('children' in item);
}

function select(item: UiMenuItem) {
    if (!isLeaf(item)) return;
    if ('disabled' in item && item.disabled) return;
    closeMenu();
    emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'string', value: item.id } });
}

async function openMenu() {
    if (!triggerEl) return;
    portalTarget = findPortalTarget(triggerEl);
    const r = triggerEl.getBoundingClientRect();
    const initialMax = window.innerHeight - (r.bottom + MENU_GAP) - VIEWPORT_PAD;
    menuPos = {
        top: r.bottom + MENU_GAP,
        left: r.left,
        minWidth: r.width,
        maxHeight: Math.max(120, initialMax),
    };
    open = true;
    await tick();
    if (menuEl && menuPos) {
        const mr = menuEl.getBoundingClientRect();
        const overflowRight = mr.right - (window.innerWidth - VIEWPORT_PAD);
        let { top, left, minWidth, maxHeight } = menuPos;
        if (overflowRight > 0) left = Math.max(VIEWPORT_PAD, left - overflowRight);

        const spaceBelow = window.innerHeight - (r.bottom + MENU_GAP) - VIEWPORT_PAD;
        const spaceAbove = r.top - MENU_GAP - VIEWPORT_PAD;
        if (mr.height > spaceBelow && spaceAbove > spaceBelow) {
            top = Math.max(VIEWPORT_PAD, r.top - MENU_GAP - Math.min(mr.height, spaceAbove));
            maxHeight = spaceAbove;
        } else {
            maxHeight = spaceBelow;
        }

        menuPos = { top, left, minWidth, maxHeight: Math.max(120, maxHeight) };
    }
}

function closeMenu() {
    open = false;
    hoveredPath = [];
    menuPos = null;
    submenuPos = null;
}

function onTriggerClick() {
    if (open) closeMenu();
    else openMenu();
}

function onDocumentClick(e: MouseEvent) {
    if (!open) return;
    const t = e.target as Node | null;
    if (triggerEl && t && triggerEl.contains(t)) return;
    if (menuEl && t && menuEl.contains(t)) return;
    if (submenuRoot && t && submenuRoot.contains(t)) return;
    closeMenu();
}

function onWindowResize() {
    if (open) closeMenu();
}

function onWindowScroll(e: Event) {
    if (!open) return;
    const t = e.target as Node | null;
    if (menuEl && t && menuEl.contains(t)) return;
    if (submenuRoot && t && submenuRoot.contains(t)) return;
    closeMenu();
}

$effect(() => {
    if (!open) return;
    document.addEventListener('mousedown', onDocumentClick);
    window.addEventListener('resize', onWindowResize);
    window.addEventListener('scroll', onWindowScroll, true);
    return () => {
        document.removeEventListener('mousedown', onDocumentClick);
        window.removeEventListener('resize', onWindowResize);
        window.removeEventListener('scroll', onWindowScroll, true);
    };
});

function pathHas(itemId: string, depth: number): boolean {
    return hoveredPath[depth] === itemId;
}

async function hoverTopLevel(itemId: string, hostEl: HTMLElement, hasChildren: boolean) {
    hoveredPath = [itemId];
    if (!hasChildren) {
        submenuPos = null;
        return;
    }
    const r = hostEl.getBoundingClientRect();
    const initialMax = window.innerHeight - r.top - VIEWPORT_PAD;
    submenuPos = { top: r.top, left: r.right + MENU_GAP, maxHeight: Math.max(120, initialMax) };
    await tick();
    if (submenuRoot && submenuPos) {
        const sr = submenuRoot.getBoundingClientRect();
        const overflowRight = sr.right - (window.innerWidth - VIEWPORT_PAD);
        let { top, left, maxHeight } = submenuPos;
        if (overflowRight > 0) {
            const flippedLeft = r.left - sr.width - MENU_GAP;
            if (flippedLeft >= VIEWPORT_PAD) left = flippedLeft;
            else left = Math.max(VIEWPORT_PAD, left - overflowRight);
        }

        const spaceBelow = window.innerHeight - top - VIEWPORT_PAD;
        if (sr.height > spaceBelow) {
            const newTop = Math.max(VIEWPORT_PAD, window.innerHeight - VIEWPORT_PAD - Math.min(sr.height, window.innerHeight - 2 * VIEWPORT_PAD));
            top = newTop;
            maxHeight = window.innerHeight - top - VIEWPORT_PAD;
        } else {
            maxHeight = spaceBelow;
        }

        submenuPos = { top, left, maxHeight: Math.max(120, maxHeight) };
    }
}

function portal(el: HTMLElement, target: HTMLElement | null) {
    (target ?? document.body).appendChild(el);
    return {
        update(newTarget: HTMLElement | null) {
            const t = newTarget ?? document.body;
            if (el.parentElement !== t) t.appendChild(el);
        },
        destroy() {
            try { el.remove(); } catch { /* noop */ }
        },
    };
}

let activeSubmenuItems = $derived.by<UiMenuItem[]>(() => {
    if (!hoveredPath[0]) return [];
    const top = node.items.find((i) => i.id === hoveredPath[0]);
    if (!top || isLeaf(top)) return [];
    return top.children;
});
</script>

<div class="relative inline-flex">
    <button
        bind:this={triggerEl}
        type="button"
        class={cls}
        onclick={onTriggerClick}
        aria-haspopup="menu"
        aria-expanded={open}
    >
        {#if node.icon}<UiIcon node={{ kind: 'icon', id: node.id + ':icon', icon: node.icon, size: 14 }} />{/if}
        <span>{node.text}</span>
    </button>
</div>

{#if open && menuPos}
    <div
        bind:this={menuEl}
        use:portal={portalTarget}
        role="menu"
        class="fixed z-[1000] bg-base-200 border border-base-content/15 rounded-md shadow-2xl overflow-y-auto overflow-x-hidden py-1"
        style:top="{menuPos.top}px"
        style:left="{menuPos.left}px"
        style:min-width="{menuPos.minWidth}px"
        style:max-height="{menuPos.maxHeight}px"
    >
        {#each node.items as item (item.id)}
            {@const hasChildren = !isLeaf(item)}
            {@const hovered = pathHas(item.id, 0)}
            <button
                type="button"
                role="menuitem"
                class="w-full flex items-center justify-between gap-2 px-3 py-1.5 text-left text-sm bg-transparent border-0 cursor-pointer transition-colors hover:bg-base-content/10
                       {hovered ? 'bg-base-content/10' : ''}"
                disabled={isLeaf(item) && 'disabled' in item && item.disabled}
                onclick={() => select(item)}
                onmouseenter={(ev) => hoverTopLevel(item.id, ev.currentTarget as HTMLElement, hasChildren)}
            >
                <span class="flex items-center gap-2">
                    {#if item.icon}<UiIcon node={{ kind: 'icon', id: item.id + ':icon', icon: item.icon, size: 12 }} />{/if}
                    {item.label}
                </span>
                {#if hasChildren}<span class="text-base-content/55 text-xs">›</span>{/if}
            </button>
        {/each}
    </div>
{/if}

{#if open && submenuPos && activeSubmenuItems.length}
    <div
        bind:this={submenuRoot}
        use:portal={portalTarget}
        role="menu"
        class="fixed z-[1001] min-w-[180px] bg-base-200 border border-base-content/15 rounded-md shadow-2xl overflow-y-auto overflow-x-hidden py-1"
        style:top="{submenuPos.top}px"
        style:left="{submenuPos.left}px"
        style:max-height="{submenuPos.maxHeight}px"
    >
        {#each activeSubmenuItems as child (child.id)}
            {@const childHovered = pathHas(child.id, 1)}
            <button
                type="button"
                role="menuitem"
                class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-sm bg-transparent border-0 cursor-pointer transition-colors hover:bg-base-content/10
                       {childHovered ? 'bg-base-content/10' : ''}"
                onmouseenter={() => { hoveredPath = [hoveredPath[0], child.id]; }}
                disabled={isLeaf(child) && 'disabled' in child && child.disabled}
                onclick={() => select(child)}
            >
                {#if child.icon}<UiIcon node={{ kind: 'icon', id: child.id + ':icon', icon: child.icon, size: 12 }} />{/if}
                {child.label}
            </button>
        {/each}
    </div>
{/if}
