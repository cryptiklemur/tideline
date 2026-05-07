import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Contributions, PermissionRequest, UiEvent } from './types';

const empty: Contributions = {
    settings_sections: [], status_pills: [], channel_overlays: [],
    tray_items: [], keybind_actions: [], iframe_surfaces: [], input_overlays: [],
};

let _contributions = $state<Contributions>(empty);
let _permissionPrompt = $state<PermissionRequest | null>(null);
let _pluginEvents = $state<Record<string, unknown>>({});
let _pluginEventsBySource = $state<Record<string, Record<string, unknown>>>({});
let unlistenContrib: UnlistenFn | null = null;
let unlistenPerm: UnlistenFn | null = null;
let unlistenEvents: UnlistenFn | null = null;

export const pluginUi = {
    get contributions() {
        return _contributions;
    },
    get permissionPrompt() {
        return _permissionPrompt;
    },
    get pluginEvents() {
        return _pluginEvents;
    },
    get pluginEventsBySource() {
        return _pluginEventsBySource;
    },
    pluginEventBySource(topic: string, sourceName: string): unknown {
        return _pluginEventsBySource[topic]?.[sourceName];
    },
    async init() {
        unlistenContrib = await listen<Contributions>('tideline-plugin:contributions', (e) => {
            _contributions = e.payload;
        });
        unlistenPerm = await listen<PermissionRequest>('tideline-plugin:permission-request', (e) => {
            _permissionPrompt = e.payload;
        });
        unlistenEvents = await listen<{ topic: string; params: unknown }>('tideline-plugin:event', (e) => {
            const { topic, params } = e.payload;
            _pluginEvents = { ..._pluginEvents, [topic]: params };
            const sourceName =
                params && typeof params === 'object' && 'source_name' in params
                    ? (params as { source_name?: unknown }).source_name
                    : undefined;
            if (typeof sourceName === 'string') {
                const prev = _pluginEventsBySource[topic] ?? {};
                _pluginEventsBySource = {
                    ..._pluginEventsBySource,
                    [topic]: { ...prev, [sourceName]: params },
                };
            }
        });
        _contributions = await invoke<Contributions>('tideline_plugin_contributions');
        try {
            await invoke('tideline_plugin_replay_states');
        } catch (e) {
            console.warn('tideline_plugin_replay_states failed', e);
        }
    },
    teardown() {
        unlistenContrib?.();
        unlistenPerm?.();
        unlistenEvents?.();
        unlistenContrib = null;
        unlistenPerm = null;
        unlistenEvents = null;
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
