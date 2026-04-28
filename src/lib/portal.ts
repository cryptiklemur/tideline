export function portal(node: HTMLElement, target: HTMLElement | string = document.body) {
    const dest = typeof target === 'string' ? document.querySelector<HTMLElement>(target) : target;
    if (!dest) return {};
    dest.appendChild(node);
    return {
        destroy() {
            if (node.parentNode === dest) dest.removeChild(node);
        },
    };
}
