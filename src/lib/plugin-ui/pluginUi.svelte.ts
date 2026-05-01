import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Contributions, PermissionRequest, UiEvent } from './types';

const empty: Contributions = {
    settings_sections: [], status_pills: [], channel_overlays: [],
    tray_items: [], keybind_actions: [], iframe_surfaces: [],
};

let _contributions = $state<Contributions>(empty);
let _permissionPrompt = $state<PermissionRequest | null>(null);
let unlistenContrib: UnlistenFn | null = null;
let unlistenPerm: UnlistenFn | null = null;

export const pluginUi = {
    get contributions() {
        return _contributions;
    },
    get permissionPrompt() {
        return _permissionPrompt;
    },
    async init() {
        _contributions = await invoke<Contributions>('tideline_plugin_contributions');
        unlistenContrib = await listen<Contributions>('tideline-plugin:contributions', (e) => {
            _contributions = e.payload;
        });
        unlistenPerm = await listen<PermissionRequest>('tideline-plugin:permission-request', (e) => {
            _permissionPrompt = e.payload;
        });
    },
    teardown() {
        unlistenContrib?.();
        unlistenPerm?.();
        unlistenContrib = null;
        unlistenPerm = null;
    },
    async emit(pluginId: string, event: UiEvent) {
        await invoke('tideline_plugin_emit_event', { pluginId, event });
    },
    async respondPermission(grant: boolean) {
        const prompt = _permissionPrompt;
        if (!prompt) return;
        _permissionPrompt = null;
        await invoke('tideline_plugin_request_permission', {
            pluginId: prompt.plugin_id,
            capability: prompt.capability,
            grant,
        });
    },
};
