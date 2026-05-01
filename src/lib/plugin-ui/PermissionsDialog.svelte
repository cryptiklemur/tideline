<script lang="ts">
import Icon from '$lib/Icon.svelte';
import type { PermissionRequest } from './types';

interface Props {
    request: PermissionRequest;
    onResolve: (grant: boolean) => void;
}

let { request, onResolve }: Props = $props();
</script>

<div class="modal modal-open">
    <div class="modal-box max-w-md">
        <h3 class="text-base font-semibold m-0 flex items-center gap-2">
            <Icon name="shield-check" size={14} />
            Plugin permission
        </h3>
        <p class="text-sm text-base-content/70 mt-2">
            <span class="font-mono">{request.plugin_id}</span> is requesting
            <span class="badge badge-warning badge-soft px-2 py-0.5 text-xs">{request.capability}</span>.
        </p>
        <p class="text-sm mt-3 leading-snug">{request.rationale}</p>
        <div class="modal-action">
            <button class="btn btn-soft btn-sm" onclick={() => onResolve(false)}>Deny</button>
            <button class="btn btn-primary btn-sm" onclick={() => onResolve(true)}>Grant</button>
        </div>
    </div>
</div>
