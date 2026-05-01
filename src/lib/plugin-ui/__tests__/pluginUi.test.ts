import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { Contributions, UiEvent } from '../types';

const invoke = vi.fn();
const listen = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

describe('pluginUi store', () => {
    beforeEach(() => {
        invoke.mockReset();
        listen.mockReset();
    });

    it('init() loads contributions and subscribes to updates', async () => {
        const empty: Contributions = {
            settings_sections: [], status_pills: [], channel_overlays: [],
            tray_items: [], keybind_actions: [], iframe_surfaces: [],
        };
        invoke.mockResolvedValueOnce(empty);
        listen.mockResolvedValueOnce(() => {});
        const { pluginUi } = await import('../pluginUi.svelte');
        await pluginUi.init();
        expect(invoke).toHaveBeenCalledWith('tideline_plugin_contributions');
        expect(listen).toHaveBeenCalledWith('tideline-plugin:contributions', expect.any(Function));
    });

    it('emit() invokes tideline_plugin_emit_event with plugin_id', async () => {
        invoke.mockResolvedValue(undefined);
        const { pluginUi } = await import('../pluginUi.svelte');
        const event: UiEvent = {
            surface_id: 'sec', node_id: 'n1',
            value: { type: 'bool', value: true },
        };
        await pluginUi.emit('plug-a', event);
        expect(invoke).toHaveBeenCalledWith('tideline_plugin_emit_event', {
            pluginId: 'plug-a', event,
        });
    });
});
