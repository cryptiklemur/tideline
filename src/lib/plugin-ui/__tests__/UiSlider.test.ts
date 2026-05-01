import { fireEvent, render } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UiSlider from '../UiSlider.svelte';

describe('UiSlider', () => {
    it('emits number event on input', async () => {
        const emit = vi.fn();
        const { container } = render(UiSlider, {
            props: {
                node: { kind: 'slider', id: 's1', min: 0, max: 100, value: 25 },
                surfaceId: 'sec',
                emit,
            },
        });
        const range = container.querySelector('input[type=range]') as HTMLInputElement;
        await fireEvent.input(range, { target: { value: '60' } });
        expect(emit).toHaveBeenCalledWith({
            surface_id: 'sec',
            node_id: 's1',
            value: { type: 'number', value: 60 },
        });
    });
});
