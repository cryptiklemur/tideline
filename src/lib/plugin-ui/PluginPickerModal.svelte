<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { onMount, tick } from 'svelte';
import Icon from '$lib/Icon.svelte';

interface CatalogItem {
    uri: string;
    name: string;
    vendor: string;
    format: string;
    category: string;
    recommended: boolean;
}

interface CatalogGroup {
    category: string;
    label: string;
    items: CatalogItem[];
}

interface CatalogResponse {
    groups: CatalogGroup[];
    recommended: CatalogItem[];
}

interface Props {
    pluginId: string;
    channelKind?: 'physical_input' | 'input' | 'output';
    onClose: () => void;
    onPick: (uri: string) => void;
}
let { pluginId, channelKind, onClose, onPick }: Props = $props();

let dlg: HTMLDialogElement | undefined = $state();
let listEl: HTMLDivElement | undefined = $state();
let inputEl: HTMLInputElement | undefined = $state();

let catalog = $state<CatalogResponse | null>(null);
let loading = $state(true);
let error = $state<string | null>(null);
let query = $state('');
let activeIndex = $state(0);

interface FlatRow {
    kind: 'header' | 'item';
    headerLabel?: string;
    headerCount?: number;
    item?: CatalogItem;
    matchScore?: number;
}

async function load() {
    try {
        const result = await invoke<CatalogResponse>('tideline_plugin_request', {
            pluginId,
            method: 'effects.list_catalog',
            params: { channel_kind: channelKind },
        });
        catalog = result;
        error = null;
    } catch (e) {
        error = String(e);
    } finally {
        loading = false;
    }
}

function scoreMatch(item: CatalogItem, q: string): number {
    if (!q) return 1;
    const ql = q.toLowerCase();
    const name = item.name.toLowerCase();
    const vendor = item.vendor.toLowerCase();
    const cat = item.category.toLowerCase();
    if (name === ql) return 1000;
    if (name.startsWith(ql)) return 800;
    if (name.includes(ql)) return 500;
    if (vendor.includes(ql)) return 250;
    if (cat.includes(ql)) return 100;
    if (item.format.toLowerCase().includes(ql)) return 80;
    return 0;
}

let rows = $derived.by<FlatRow[]>(() => {
    if (!catalog) return [];
    const q = query.trim();
    const out: FlatRow[] = [];

    if (!q) {
        if (catalog.recommended.length) {
            out.push({ kind: 'header', headerLabel: 'Recommended', headerCount: catalog.recommended.length });
            for (const item of catalog.recommended) out.push({ kind: 'item', item });
        }
        for (const group of catalog.groups) {
            const items = group.items.filter((it) => !it.recommended);
            if (!items.length) continue;
            out.push({ kind: 'header', headerLabel: group.label, headerCount: items.length });
            for (const item of items) out.push({ kind: 'item', item });
        }
    } else {
        const seen = new Set<string>();
        const allItems: { item: CatalogItem; score: number }[] = [];
        for (const item of catalog.recommended) {
            if (seen.has(item.uri)) continue;
            const score = scoreMatch(item, q);
            if (score > 0) { allItems.push({ item, score }); seen.add(item.uri); }
        }
        for (const group of catalog.groups) {
            for (const item of group.items) {
                if (seen.has(item.uri)) continue;
                const score = scoreMatch(item, q);
                if (score > 0) { allItems.push({ item, score }); seen.add(item.uri); }
            }
        }
        allItems.sort((a, b) => b.score - a.score || a.item.name.localeCompare(b.item.name));
        out.push({ kind: 'header', headerLabel: `${allItems.length} match${allItems.length === 1 ? '' : 'es'}`, headerCount: allItems.length });
        for (const { item, score } of allItems) out.push({ kind: 'item', item, matchScore: score });
    }

    return out;
});

let itemRows = $derived(rows.filter((r) => r.kind === 'item'));

$effect(() => {
    void rows;
    activeIndex = Math.min(activeIndex, Math.max(0, itemRows.length - 1));
});

async function ensureActiveVisible() {
    await tick();
    if (!listEl) return;
    const node = listEl.querySelector<HTMLElement>(`[data-active="true"]`);
    if (!node) return;
    const lr = listEl.getBoundingClientRect();
    const nr = node.getBoundingClientRect();
    if (nr.top < lr.top) listEl.scrollTop -= lr.top - nr.top;
    else if (nr.bottom > lr.bottom) listEl.scrollTop += nr.bottom - lr.bottom;
}

function selectIdx(idx: number) {
    const row = itemRows[idx];
    if (!row?.item) return;
    onPick(row.item.uri);
}

function onKeydown(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
        ev.preventDefault();
        onClose();
        return;
    }
    if (ev.key === 'ArrowDown') {
        ev.preventDefault();
        if (itemRows.length === 0) return;
        activeIndex = (activeIndex + 1) % itemRows.length;
        void ensureActiveVisible();
        return;
    }
    if (ev.key === 'ArrowUp') {
        ev.preventDefault();
        if (itemRows.length === 0) return;
        activeIndex = (activeIndex - 1 + itemRows.length) % itemRows.length;
        void ensureActiveVisible();
        return;
    }
    if (ev.key === 'Home') {
        ev.preventDefault();
        activeIndex = 0;
        void ensureActiveVisible();
        return;
    }
    if (ev.key === 'End') {
        ev.preventDefault();
        activeIndex = Math.max(0, itemRows.length - 1);
        void ensureActiveVisible();
        return;
    }
    if (ev.key === 'Enter') {
        ev.preventDefault();
        selectIdx(activeIndex);
        return;
    }
}

function onBackdropClick(ev: MouseEvent) {
    if (ev.target === dlg) onClose();
}

onMount(async () => {
    dlg?.showModal();
    await load();
    await tick();
    inputEl?.focus();
});

function fmtBadgeClass(fmt: string): string {
    const f = fmt.toLowerCase();
    if (f === 'lv2') return 'badge-soft badge-info';
    if (f === 'vst3') return 'badge-soft badge-success';
    if (f === 'vst2') return 'badge-soft badge-warning';
    if (f === 'clap') return 'badge-soft badge-secondary';
    return 'badge-ghost';
}
</script>

<dialog
    bind:this={dlg}
    class="modal"
    onclick={onBackdropClick}
    onkeydown={onKeydown}
>
    <div class="modal-box max-w-2xl w-full p-0 bg-base-200 border border-base-content/15 rounded-xl shadow-2xl flex flex-col max-h-[calc(100dvh-4rem)]">
        <header class="flex items-center gap-3 px-4 py-3 border-b border-base-content/15 shrink-0">
            <Icon name="plus" size={16} />
            <input
                bind:this={inputEl}
                bind:value={query}
                type="text"
                placeholder="Search effects… (name, vendor, category, format)"
                class="input input-ghost flex-1 text-sm focus:outline-none focus:bg-transparent"
                autocomplete="off"
                spellcheck="false"
            />
            <kbd class="kbd kbd-xs hidden sm:inline-flex">esc</kbd>
            <button
                type="button"
                class="btn btn-ghost btn-sm btn-square"
                onclick={onClose}
                aria-label="Close"
            >
                <Icon name="close" size={16} />
            </button>
        </header>

        <div bind:this={listEl} class="flex-1 min-h-[280px] max-h-[60dvh] overflow-y-auto bg-base-100">
            {#if loading}
                <div class="flex items-center justify-center py-16">
                    <span class="loading loading-spinner loading-md text-primary"></span>
                </div>
            {:else if error}
                <div class="alert alert-error m-4 text-xs items-start gap-2" role="alert">
                    <Icon name="alert" size={14} />
                    <div class="flex flex-col gap-1">
                        <span class="font-semibold">Couldn't load plugin catalog</span>
                        <span class="opacity-80">{error}</span>
                    </div>
                </div>
            {:else if itemRows.length === 0}
                <div class="flex flex-col items-center justify-center py-16 gap-2 text-base-content/60">
                    <Icon name="package-open" size={32} />
                    <span class="text-sm">No plugins match "{query}"</span>
                </div>
            {:else}
                {@const flatItemIndex = (() => {
                    const m = new Map<number, number>();
                    let idx = 0;
                    rows.forEach((r, i) => {
                        if (r.kind === 'item') { m.set(i, idx); idx++; }
                    });
                    return m;
                })()}
                {#each rows as row, i (i + ':' + (row.item?.uri ?? row.headerLabel ?? ''))}
                    {#if row.kind === 'header'}
                        <div class="sticky top-0 z-10 px-4 py-1.5 bg-base-200/95 backdrop-blur border-b border-base-content/10 text-[10px] font-bold uppercase tracking-widest text-base-content/55 flex items-center justify-between">
                            <span>{row.headerLabel}</span>
                            <span class="text-base-content/40 normal-case font-medium tracking-normal">{row.headerCount}</span>
                        </div>
                    {:else if row.item}
                        {@const itemIdx = flatItemIndex.get(i) ?? 0}
                        {@const isActive = itemIdx === activeIndex}
                        <button
                            type="button"
                            data-active={isActive}
                            class="w-full flex items-center gap-3 px-4 py-2 text-left text-sm bg-transparent border-0 cursor-pointer transition-colors hover:bg-base-content/5
                                   {isActive ? 'bg-primary/15 hover:bg-primary/15' : ''}"
                            onmouseenter={() => { activeIndex = itemIdx; }}
                            onclick={() => selectIdx(itemIdx)}
                        >
                            <span class="size-6 inline-flex items-center justify-center rounded-md shrink-0 {row.item.recommended ? 'text-warning' : 'text-base-content/40'}">
                                {#if row.item.recommended}
                                    <Icon name="star" size={14} />
                                {:else}
                                    <Icon name="fx" size={14} />
                                {/if}
                            </span>
                            <span class="flex flex-col min-w-0 flex-1">
                                <span class="font-medium truncate">{row.item.name}</span>
                                <span class="text-xs text-base-content/55 truncate">
                                    {row.item.vendor || row.item.category}
                                </span>
                            </span>
                            <span class="badge badge-xs {fmtBadgeClass(row.item.format)} shrink-0">
                                {row.item.format}
                            </span>
                        </button>
                    {/if}
                {/each}
            {/if}
        </div>

        <footer class="flex items-center justify-between gap-3 px-4 py-2 border-t border-base-content/15 shrink-0 text-xs text-base-content/55">
            <div class="flex items-center gap-2">
                <kbd class="kbd kbd-xs">↑</kbd>
                <kbd class="kbd kbd-xs">↓</kbd>
                <span>navigate</span>
            </div>
            <div class="flex items-center gap-2">
                <kbd class="kbd kbd-xs">↵</kbd>
                <span>add to chain</span>
            </div>
            <div class="flex items-center gap-2">
                <span>{itemRows.length} of {catalog ? catalog.groups.reduce((n, g) => n + g.items.length, 0) : 0}</span>
            </div>
        </footer>
    </div>
</dialog>
