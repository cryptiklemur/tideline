<script lang="ts">
import BindingCapture from '$lib/BindingCapture.svelte';
import type { Binding } from '$lib/types';
import type { UiEvent, UiNode } from './types';
interface Props {
    node: Extract<UiNode, { kind: 'binding_capture' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
function set(b: Binding) {
    emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'binding', value: b } });
}
function clear() {
    emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'binding', value: null } });
}
</script>

<BindingCapture
    label={node.label}
    sublabel={node.sublabel}
    binding={node.binding}
    onSet={set}
    onClear={clear}
/>
