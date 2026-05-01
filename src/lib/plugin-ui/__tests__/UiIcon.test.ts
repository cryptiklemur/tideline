import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import UiIcon from '../UiIcon.svelte';

describe('UiIcon', () => {
    it('renders fallback for unknown name', () => {
        render(UiIcon, { props: { node: { kind: 'icon', id: 'i', icon: { name: 'definitely-not-a-real-icon' } } } });
        expect(screen.getByTestId('ui-icon-fallback')).toBeInTheDocument();
    });
});
