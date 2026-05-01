import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UiButton from '../UiButton.svelte';

describe('UiButton', () => {
    it('emits click', async () => {
        const emit = vi.fn();
        render(UiButton, {
            props: {
                node: { kind: 'button', id: 'b1', text: 'Run', variant: 'soft' },
                surfaceId: 'sec',
                emit,
            },
        });
        await fireEvent.click(screen.getByRole('button', { name: 'Run' }));
        expect(emit).toHaveBeenCalledWith({
            surface_id: 'sec',
            node_id: 'b1',
            value: { type: 'click' },
        });
    });
});
