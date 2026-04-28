<script lang="ts" module>
import { ICONS, type IconName } from './Icon.svelte';
import type { ChannelKind } from './types';

export const CHANNEL_ICON_OPTIONS: IconName[] = [
    'speaker', 'headphones', 'volume', 'volume-low',
    'mic', 'mic-vocal',
    'wave', 'audio-lines',
    'music', 'music-2', 'music-3', 'music-4',
    'gamepad', 'globe', 'chat', 'video', 'tv',
    'radio', 'podcast', 'megaphone', 'film',
    'disc', 'disc-3', 'monitor', 'bell', 'party',
];

export function defaultIconForKind(kind: ChannelKind): IconName {
    if (kind === 'physical_input') return 'mic';
    if (kind === 'input') return 'wave';
    return 'speaker';
}

export function isLucideName(value: string): value is IconName {
    return value in ICONS;
}
</script>

<script lang="ts">
import Icon from './Icon.svelte';

interface Props {
    icon: string;
    kind: ChannelKind;
    size?: number;
    programs?: string[];
    appIcons?: Record<string, string | null>;
}

let { icon, kind, size = 14, programs = [], appIcons = {} }: Props = $props();

const isCustom = $derived(typeof icon === 'string' && icon.startsWith('data:'));
const isAuto = $derived(!isCustom && !icon);

const resolvedAppIcons = $derived(
    isAuto && (kind === 'output' || !kind)
        ? programs.map(p => appIcons[p] ?? null).filter((v): v is string => !!v).slice(0, 4)
        : []
);

const useAppGrid = $derived(resolvedAppIcons.length > 0);

const lucideName = $derived<IconName>(
    isCustom || isAuto
        ? defaultIconForKind(kind)
        : isLucideName(icon)
            ? icon
            : defaultIconForKind(kind),
);

const pad = $derived(Math.max(2, Math.round(size * 0.14)));
const gap = $derived(Math.max(1, Math.round(size * 0.07)));
const radius = $derived(Math.max(2, Math.round(size * 0.12)));
</script>

{#if isCustom}
    <img class="object-contain rounded-sm block" src={icon} alt="" style:width="{size}px" style:height="{size}px" />
{:else if useAppGrid}
    {#if resolvedAppIcons.length === 1}
        <div
            class="bg-base-100/60 flex items-center justify-center"
            style:width="{size}px"
            style:height="{size}px"
            style:padding="{pad}px"
            style:border-radius="{radius}px"
        >
            <img class="w-full h-full object-contain block" src={resolvedAppIcons[0]} alt="" />
        </div>
    {:else}
        {@const cols = resolvedAppIcons.length === 2 ? 'grid-cols-2 grid-rows-1' : 'grid-cols-2 grid-rows-2'}
        <div
            class="grid {cols} bg-base-100/60"
            style:width="{size}px"
            style:height="{size}px"
            style:padding="{pad}px"
            style:gap="{gap}px"
            style:border-radius="{radius}px"
        >
            {#each resolvedAppIcons as src, i (i)}
                <img class="w-full h-full object-contain block min-w-0 min-h-0" {src} alt="" />
            {/each}
            {#if resolvedAppIcons.length === 3}
                <div></div>
            {/if}
        </div>
    {/if}
{:else}
    <Icon name={lucideName} {size} />
{/if}
