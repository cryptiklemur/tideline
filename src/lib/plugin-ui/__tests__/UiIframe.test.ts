import { render } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

const { listen, invoke } = vi.hoisted(() => ({
    listen: vi.fn(async () => () => {}),
    invoke: vi.fn(async () => undefined),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import UiIframe from '../UiIframe.svelte';

describe('UiIframe', () => {
    it('builds correct src and sandbox attrs', () => {
        const { container } = render(UiIframe, {
            props: {
                node: { kind: 'iframe', id: 'if1', src_id: 'panel', height: 240 },
                pluginId: 'plug-a',
                surfaceId: 'panel',
            },
        });
        const iframe = container.querySelector('iframe') as HTMLIFrameElement;
        expect(iframe).not.toBeNull();
        expect(iframe.src).toContain('tideline-plugin://plug-a/panel/');
        expect(iframe.getAttribute('sandbox')).toBe('allow-scripts');
        expect(iframe.style.height).toBe('240px');
    });
});
