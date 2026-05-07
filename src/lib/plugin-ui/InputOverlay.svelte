<script lang="ts">
import type { InputOverlayContribution, UiEvent, UiNode } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    overlay: InputOverlayContribution;
    sourceName: string;
    emit: (e: UiEvent) => void;
}
let { overlay, sourceName, emit }: Props = $props();

function emitWithContext(e: UiEvent) {
    emit({ ...e, context: { kind: 'input', source_name: sourceName } });
}

function applyOverrides(node: UiNode, overrides: Record<string, unknown>): UiNode {
    if (!overrides || Object.keys(overrides).length === 0) return node;
    const idOf = (n: UiNode): string | undefined => (n as unknown as { id?: string }).id;
    const id = idOf(node);
    let next: UiNode = node;
    if (id && id in overrides) {
        const override = overrides[id];
        const kind = (node as { kind?: string }).kind;
        if (kind === 'binding_capture') {
            next = { ...(node as object), binding: override } as UiNode;
        } else {
            next = { ...(node as object), value: override } as UiNode;
        }
    }
    const children = (next as unknown as { children?: UiNode[] }).children;
    if (Array.isArray(children)) {
        return {
            ...(next as object),
            children: children.map((c) => applyOverrides(c, overrides)),
        } as UiNode;
    }
    return next;
}

let renderedNode = $derived(
    applyOverrides(overlay.tree, overlay.values_by_source?.[sourceName] ?? {}),
);
</script>

<div
    class="flex flex-col gap-1"
    data-plugin-id={overlay.plugin_id}
    data-surface-id={overlay.surface_id}
    data-source-name={sourceName}
>
    <UiTree node={renderedNode} surfaceId={overlay.surface_id} pluginId={overlay.plugin_id} emit={emitWithContext} />
</div>
