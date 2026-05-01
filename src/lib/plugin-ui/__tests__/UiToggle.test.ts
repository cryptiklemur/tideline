import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UiToggle from '../UiToggle.svelte';

describe('UiToggle', () => {
    it('emits bool event when clicked', async () => {
        const emit = vi.fn();
        render(UiToggle, {
            props: {
                node: { kind: 'toggle', id: 't1', label: 'Enable foo', value: false },
                surfaceId: 'sec',
                emit,
            },
        });
        const cb = screen.getByRole('checkbox');
        await fireEvent.click(cb);
        expect(emit).toHaveBeenCalledWith({
            surface_id: 'sec',
            node_id: 't1',
            value: { type: 'bool', value: true },
        });
    });
});
