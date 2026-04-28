<script lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window';

type Direction = 'North' | 'South' | 'East' | 'West' | 'NorthEast' | 'NorthWest' | 'SouthEast' | 'SouthWest';

function start(dir: Direction) {
    return (e: MouseEvent) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.stopPropagation();
        getCurrentWindow().startResizeDragging(dir).catch(err => console.error('resize failed:', err));
    };
}

const edge = 'fixed z-[200] bg-transparent transition-colors hover:bg-primary/20';
const corner = 'fixed z-[200] bg-transparent transition-colors hover:bg-primary/20 w-2 h-2';
</script>

<div class="{edge} top-0 left-1.5 right-1.5 h-1 cursor-n-resize" onmousedown={start('North')} role="presentation"></div>
<div class="{edge} bottom-0 left-1.5 right-1.5 h-1 cursor-s-resize" onmousedown={start('South')} role="presentation"></div>
<div class="{edge} left-0 top-1.5 bottom-1.5 w-1 cursor-w-resize" onmousedown={start('West')} role="presentation"></div>
<div class="{edge} right-0 top-1.5 bottom-1.5 w-1 cursor-e-resize" onmousedown={start('East')} role="presentation"></div>
<div class="{corner} top-0 left-0 cursor-nw-resize" onmousedown={start('NorthWest')} role="presentation"></div>
<div class="{corner} top-0 right-0 cursor-ne-resize" onmousedown={start('NorthEast')} role="presentation"></div>
<div class="{corner} bottom-0 left-0 cursor-sw-resize" onmousedown={start('SouthWest')} role="presentation"></div>
<div class="{corner} bottom-0 right-0 cursor-se-resize" onmousedown={start('SouthEast')} role="presentation"></div>
