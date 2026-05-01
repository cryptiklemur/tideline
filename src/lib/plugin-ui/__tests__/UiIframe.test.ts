import { render } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

const { listen, invoke } = vi.hoisted(() => ({
    listen: vi.fn<(topic: string, fn: (e: { payload: unknown }) => void) => Promise<() => void>>(
        async () => () => {},
    ),
    invoke: vi.fn<(...args: unknown[]) => Promise<unknown>>(async () => undefined),
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

    it('forwards host->iframe relay events through postMessage', async () => {
        const capturedRef: { fn: ((event: { payload: unknown }) => void) | null } = { fn: null };
        listen.mockImplementationOnce(async (_topic: string, fn: (e: { payload: unknown }) => void) => {
            capturedRef.fn = fn;
            return () => {};
        });
        const post = vi.fn();
        Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
            configurable: true,
            get() {
                return { postMessage: post } as unknown as Window;
            },
        });

        render(UiIframe, {
            props: {
                node: { kind: 'iframe', id: 'if1', src_id: 'panel' },
                pluginId: 'plug-a',
                surfaceId: 'panel',
            },
        });
        await Promise.resolve();
        await Promise.resolve();
        capturedRef.fn?.({ payload: { ping: 1 } });
        expect(post).toHaveBeenCalledWith({ __tideline: true, payload: { ping: 1 } }, '*');
    });
});
