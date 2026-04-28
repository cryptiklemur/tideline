export type ToastKind = 'info' | 'success' | 'warning' | 'error';

export interface ToastInit {
    kind?: ToastKind;
    title: string;
    body?: string;
    timeoutMs?: number;
}

export interface Toast extends Required<Omit<ToastInit, 'body'>> {
    id: number;
    body?: string;
}

class ToastStore {
    items = $state<Toast[]>([]);
    private nextId = 1;

    push(t: ToastInit): number {
        const id = this.nextId++;
        const toast: Toast = {
            id,
            kind: t.kind ?? 'info',
            title: t.title,
            body: t.body,
            timeoutMs: t.timeoutMs ?? 5000,
        };
        this.items.push(toast);
        return id;
    }

    dismiss(id: number) {
        const idx = this.items.findIndex(t => t.id === id);
        if (idx >= 0) this.items.splice(idx, 1);
    }
}

export const toaster = new ToastStore();
