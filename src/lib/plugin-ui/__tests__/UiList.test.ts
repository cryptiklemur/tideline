import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UiList from '../UiList.svelte';

describe('UiList', () => {
    it('renders items in order', () => {
        render(UiList, {
            props: {
                node: {
                    kind: 'list',
                    id: 'l1',
                    items: [
                        { id: 'a', label: 'Alpha' },
                        { id: 'b', label: 'Beta' },
                    ],
                },
                surfaceId: 'sec',
                emit: vi.fn(),
            },
        });
        expect(screen.getByText('Alpha')).toBeInTheDocument();
        expect(screen.getByText('Beta')).toBeInTheDocument();
    });
});
