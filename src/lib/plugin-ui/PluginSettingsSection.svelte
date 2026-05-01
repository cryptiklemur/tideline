<script lang="ts">
import type { SettingsSectionContribution, UiEvent } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    section: SettingsSectionContribution;
    emit: (e: UiEvent) => void;
}
let { section, emit }: Props = $props();
</script>

<div data-plugin-id={section.plugin_id} data-surface-id={section.surface_id} class="flex flex-col">
    <UiTree node={section.tree} surfaceId={section.surface_id} pluginId={section.plugin_id} {emit} />
</div>
