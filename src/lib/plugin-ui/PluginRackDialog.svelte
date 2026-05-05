<script lang="ts">
import { onMount } from 'svelte';
import UiIframe from './UiIframe.svelte';
import { pluginUi } from './pluginUi.svelte';

interface Props {
    pluginId: string;
    channelUuid: string;
    onClose: () => void;
}
let { pluginId, channelUuid, onClose }: Props = $props();

let dlg: HTMLDialogElement | undefined = $state();

let surface = $derived(
    pluginUi.contributions.iframe_surfaces.find((s) => s.plugin_id === pluginId),
);

onMount(() => {
    dlg?.showModal();
});

function onCancel(e: Event) {
    e.preventDefault();
    onClose();
}

function onBackdropClick(e: MouseEvent) {
    if (e.target === dlg) onClose();
}
</script>

<dialog
    bind:this={dlg}
    class="modal"
    oncancel={onCancel}
    onclick={onBackdropClick}
>
    <div class="modal-box max-w-4xl w-full p-0 bg-base-200 border border-base-content/15">
        <header class="flex items-center justify-between gap-3 px-4 py-2.5 border-b border-base-content/15">
            <h2 class="m-0 text-sm font-bold tracking-widest uppercase text-base-content/70">
                Effects rack
            </h2>
            <button
                type="button"
                class="btn btn-ghost btn-sm btn-square"
                onclick={onClose}
                aria-label="Close"
            >
                ✕
            </button>
        </header>
        <div class="p-0 bg-base-100">
            {#if surface}
                <UiIframe
                    node={{ kind: 'iframe', id: 'rack', src_id: surface.surface_id, height: 600 }}
                    {pluginId}
                    surfaceId={surface.surface_id}
                />
            {:else}
                <div class="p-6 text-sm text-base-content/55">
                    No rack surface registered for {pluginId}.
                </div>
            {/if}
        </div>
        <footer class="px-4 py-2 border-t border-base-content/15 text-[11px] text-base-content/55 font-mono">
            channel: {channelUuid}
        </footer>
    </div>
</dialog>
