import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UiTree from '../UiTree.svelte';
import type { UiNode } from '../types';

describe('UiTree', () => {
    it('renders heading + label', () => {
        const node: UiNode = {
            kind: 'section',
            id: 's1',
            title: 'Hello',
            children: [
                { kind: 'heading', id: 'h1', text: 'Header A' },
                { kind: 'label', id: 'l1', text: 'Body text' },
            ],
        };
        render(UiTree, { props: { node, surfaceId: 'sec', pluginId: 'p', emit: vi.fn() } });
        expect(screen.getByText('Header A')).toBeInTheDocument();
        expect(screen.getByText('Body text')).toBeInTheDocument();
    });

    it('falls back to placeholder for unknown kind', () => {
        const node = { kind: 'banana', id: 'x' } as unknown as UiNode;
        render(UiTree, { props: { node, surfaceId: 'sec', pluginId: 'p', emit: vi.fn() } });
        expect(screen.getByTestId('ui-unknown-kind')).toHaveTextContent('banana');
    });
});
