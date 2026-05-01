import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UiSelect from '../UiSelect.svelte';

describe('UiSelect', () => {
    it('emits string on change', async () => {
        const emit = vi.fn();
        render(UiSelect, {
            props: {
                node: {
                    kind: 'select',
                    id: 'sel',
                    value: 'a',
                    options: [
                        { value: 'a', label: 'A' },
                        { value: 'b', label: 'B' },
                    ],
                },
                surfaceId: 'sec',
                emit,
            },
        });
        await fireEvent.change(screen.getByRole('combobox'), { target: { value: 'b' } });
        expect(emit).toHaveBeenCalledWith({
            surface_id: 'sec',
            node_id: 'sel',
            value: { type: 'string', value: 'b' },
        });
    });
});
