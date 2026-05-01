<script lang="ts">
import type { StatusPillContribution } from './types';
import UiIcon from './UiIcon.svelte';

interface Props { pill: StatusPillContribution; }
let { pill }: Props = $props();

let cls = $derived(
    pill.tone === 'success' ? 'badge badge-success badge-soft' :
    pill.tone === 'warning' ? 'badge badge-warning badge-soft' :
    pill.tone === 'error' ? 'badge badge-error badge-soft' :
    pill.tone === 'info' ? 'badge badge-info badge-soft' :
    'badge badge-soft'
);
</script>

<span class="{cls} px-2 py-0.5 text-xs flex items-center gap-1" title={pill.tooltip ?? ''}>
    {#if pill.icon}<UiIcon node={{ kind: 'icon', id: pill.surface_id + ':icon', icon: pill.icon, size: 10 }} />{/if}
    {pill.label}
</span>
