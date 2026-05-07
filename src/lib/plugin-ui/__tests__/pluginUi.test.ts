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
    tray_items: [], keybind_actions: [], iframe_surfaces: [], input_overlays: [],
        };
        invoke.mockResolvedValueOnce(empty);
        listen.mockResolvedValueOnce(() => {});
        const { pluginUi } = await import('../pluginUi.svelte');
        await pluginUi.init();
        expect(invoke).toHaveBeenCalledWith('tideline_plugin_contributions');
        expect(listen).toHaveBeenCalledWith('tideline-plugin:contributions', expect.any(Function));
    });

    it('init() routes source-tagged events into pluginEventsBySource', async () => {
        const empty: Contributions = {
            settings_sections: [], status_pills: [], channel_overlays: [],
            tray_items: [], keybind_actions: [], iframe_surfaces: [], input_overlays: [],
        };
        invoke.mockResolvedValueOnce(empty);
        const handlers: Record<string, (e: { payload: unknown }) => void> = {};
        listen.mockImplementation((topic: string, cb: (e: { payload: unknown }) => void) => {
            handlers[topic] = cb;
            return Promise.resolve(() => {});
        });
        const { pluginUi } = await import('../pluginUi.svelte');
        await pluginUi.init();
        const cb = handlers['tideline-plugin:event'];
        expect(cb).toBeTypeOf('function');
        cb({
            payload: {
                topic: 'tideline-ptt:state_changed',
                params: { mode: 'ptt', source_name: 'mic-A', transmitting: false },
            },
        });
        cb({
            payload: {
                topic: 'tideline-ptt:state_changed',
                params: { mode: 'open', source_name: 'mic-B', transmitting: true },
            },
        });
        const a = pluginUi.pluginEventBySource('tideline-ptt:state_changed', 'mic-A') as
            | { mode?: string }
            | undefined;
        const b = pluginUi.pluginEventBySource('tideline-ptt:state_changed', 'mic-B') as
            | { mode?: string }
            | undefined;
        expect(a?.mode).toBe('ptt');
        expect(b?.mode).toBe('open');
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
