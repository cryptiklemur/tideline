import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import PermissionsDialog from '../PermissionsDialog.svelte';

describe('PermissionsDialog', () => {
    it('calls onResolve(true) when Grant clicked', async () => {
        const onResolve = vi.fn();
        render(PermissionsDialog, {
            props: {
                request: { plugin_id: 'p', capability: 'audio.read', rationale: 'read levels' },
                onResolve,
            },
        });
        await fireEvent.click(screen.getByRole('button', { name: /grant/i }));
        expect(onResolve).toHaveBeenCalledWith(true);
    });

    it('calls onResolve(false) when Deny clicked', async () => {
        const onResolve = vi.fn();
        render(PermissionsDialog, {
            props: {
                request: { plugin_id: 'p', capability: 'audio.read', rationale: 'read levels' },
                onResolve,
            },
        });
        await fireEvent.click(screen.getByRole('button', { name: /deny/i }));
        expect(onResolve).toHaveBeenCalledWith(false);
    });
});
