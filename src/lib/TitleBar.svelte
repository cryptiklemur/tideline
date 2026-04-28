<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import Icon from './Icon.svelte';

function startDrag(e: MouseEvent) {
    if (e.button !== 0) return;
    invoke('window_drag').catch(err => console.error('window_drag failed:', err));
}

function doMinimize() {
    invoke('window_minimize').catch(err => console.error('window_minimize failed:', err));
}

function doHide() {
    invoke('window_hide').catch(err => console.error('window_hide failed:', err));
}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
    class="flex items-center justify-between h-9 pl-3 pr-2 bg-base-200 border-b border-base-content/10 select-none flex-shrink-0"
    onmousedown={startDrag}
>
    <div class="flex items-center gap-2">
        <span class="flex items-center justify-center text-primary"><Icon name="wave" size={14} /></span>
        <span class="text-base font-semibold text-base-content/55 uppercase tracking-wider">Tideline</span>
    </div>
    <div class="flex gap-1" onmousedown={(e) => e.stopPropagation()}>
        <button
            class="btn btn-ghost btn-square h-7 min-h-7 w-7 text-base-content/55 hover:text-base-content"
            onclick={doMinimize}
            aria-label="Minimize"
            title="Minimize"
        >
            <Icon name="minimize" size={12} />
        </button>
        <button
            class="btn btn-ghost btn-square h-7 min-h-7 w-7 text-base-content/55 hover:bg-error/20 hover:text-error"
            onclick={doHide}
            aria-label="Close to tray"
            title="Close to tray"
        >
            <Icon name="close" size={12} />
        </button>
    </div>
</div>
