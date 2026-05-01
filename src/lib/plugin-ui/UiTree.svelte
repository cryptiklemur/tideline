<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiSection from './UiSection.svelte';
import UiRow from './UiRow.svelte';
import UiCol from './UiCol.svelte';
import UiLabel from './UiLabel.svelte';
import UiHeading from './UiHeading.svelte';
import UiIcon from './UiIcon.svelte';
import UiBadge from './UiBadge.svelte';
import UiToggle from './UiToggle.svelte';
import UiSlider from './UiSlider.svelte';
import UiInput from './UiInput.svelte';
import UiSelect from './UiSelect.svelte';
import UiButton from './UiButton.svelte';
import UiList from './UiList.svelte';
import UiBindingCapture from './UiBindingCapture.svelte';
import UiBanner from './UiBanner.svelte';
import UiDivider from './UiDivider.svelte';
import UiSpacer from './UiSpacer.svelte';
import UiIconPicker from './UiIconPicker.svelte';
import UiIframe from './UiIframe.svelte';

interface Props {
    node: UiNode;
    surfaceId: string;
    pluginId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, pluginId, emit }: Props = $props();
</script>

{#if node.kind === 'section'}
    <UiSection {node} {surfaceId} {pluginId} {emit} />
{:else if node.kind === 'row'}
    <UiRow {node} {surfaceId} {pluginId} {emit} />
{:else if node.kind === 'col'}
    <UiCol {node} {surfaceId} {pluginId} {emit} />
{:else if node.kind === 'label'}
    <UiLabel {node} />
{:else if node.kind === 'heading'}
    <UiHeading {node} />
{:else if node.kind === 'icon'}
    <UiIcon {node} />
{:else if node.kind === 'badge'}
    <UiBadge {node} />
{:else if node.kind === 'toggle'}
    <UiToggle {node} {surfaceId} {emit} />
{:else if node.kind === 'slider'}
    <UiSlider {node} {surfaceId} {emit} />
{:else if node.kind === 'input'}
    <UiInput {node} {surfaceId} {emit} />
{:else if node.kind === 'select'}
    <UiSelect {node} {surfaceId} {emit} />
{:else if node.kind === 'button'}
    <UiButton {node} {surfaceId} {emit} />
{:else if node.kind === 'list'}
    <UiList {node} {surfaceId} {emit} />
{:else if node.kind === 'binding_capture'}
    <UiBindingCapture {node} {surfaceId} {emit} />
{:else if node.kind === 'banner'}
    <UiBanner {node} />
{:else if node.kind === 'divider'}
    <UiDivider />
{:else if node.kind === 'spacer'}
    <UiSpacer {node} />
{:else if node.kind === 'icon_picker'}
    <UiIconPicker {node} {surfaceId} {emit} />
{:else if node.kind === 'iframe'}
    <UiIframe {node} {pluginId} {surfaceId} />
{:else}
    <span data-testid="ui-unknown-kind" class="text-xs text-error">unknown:{(node as { kind: string }).kind}</span>
{/if}
