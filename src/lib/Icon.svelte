<script lang="ts" module>
import Headphones from '@lucide/svelte/icons/headphones';
import Speaker from '@lucide/svelte/icons/speaker';
import Volume from '@lucide/svelte/icons/volume';
import Volume2 from '@lucide/svelte/icons/volume-2';
import VolumeX from '@lucide/svelte/icons/volume-x';
import Split from '@lucide/svelte/icons/split';
import Settings from '@lucide/svelte/icons/settings';
import X from '@lucide/svelte/icons/x';
import Plus from '@lucide/svelte/icons/plus';
import Link from '@lucide/svelte/icons/link';
import Unlink from '@lucide/svelte/icons/unlink';
import Check from '@lucide/svelte/icons/check';
import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
import AudioWaveform from '@lucide/svelte/icons/audio-waveform';
import AudioLines from '@lucide/svelte/icons/audio-lines';
import Minus from '@lucide/svelte/icons/minus';
import Mic from '@lucide/svelte/icons/mic';
import MicVocal from '@lucide/svelte/icons/mic-vocal';
import ChevronDown from '@lucide/svelte/icons/chevron-down';
import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
import Music from '@lucide/svelte/icons/music';
import Music2 from '@lucide/svelte/icons/music-2';
import Music3 from '@lucide/svelte/icons/music-3';
import Music4 from '@lucide/svelte/icons/music-4';
import Gamepad2 from '@lucide/svelte/icons/gamepad-2';
import Globe from '@lucide/svelte/icons/globe';
import MessageCircle from '@lucide/svelte/icons/message-circle';
import Video from '@lucide/svelte/icons/video';
import Tv from '@lucide/svelte/icons/tv';
import Radio from '@lucide/svelte/icons/radio';
import Podcast from '@lucide/svelte/icons/podcast';
import Megaphone from '@lucide/svelte/icons/megaphone';
import Film from '@lucide/svelte/icons/film';
import Disc from '@lucide/svelte/icons/disc';
import Disc3 from '@lucide/svelte/icons/disc-3';
import Monitor from '@lucide/svelte/icons/monitor';
import Bell from '@lucide/svelte/icons/bell';
import PartyPopper from '@lucide/svelte/icons/party-popper';
import Image from '@lucide/svelte/icons/image';
import LoaderCircle from '@lucide/svelte/icons/loader-circle';
import GripVertical from '@lucide/svelte/icons/grip-vertical';
import Keyboard from '@lucide/svelte/icons/keyboard';
import Palette from '@lucide/svelte/icons/palette';
import Info from '@lucide/svelte/icons/info';
import Cable from '@lucide/svelte/icons/cable';
import ShieldCheck from '@lucide/svelte/icons/shield-check';
import WandSparkles from '@lucide/svelte/icons/wand-sparkles';

export const ICONS = {
    headphones: Headphones,
    speaker: Speaker,
    volume: Volume2,
    'volume-low': Volume,
    'volume-mute': VolumeX,
    split: Split,
    settings: Settings,
    close: X,
    plus: Plus,
    link: Link,
    unlink: Unlink,
    check: Check,
    alert: TriangleAlert,
    wave: AudioWaveform,
    'audio-lines': AudioLines,
    minimize: Minus,
    mic: Mic,
    'mic-vocal': MicVocal,
    'chevron-down': ChevronDown,
    mixer: SlidersHorizontal,
    music: Music,
    'music-2': Music2,
    'music-3': Music3,
    'music-4': Music4,
    gamepad: Gamepad2,
    globe: Globe,
    chat: MessageCircle,
    video: Video,
    tv: Tv,
    radio: Radio,
    podcast: Podcast,
    megaphone: Megaphone,
    film: Film,
    disc: Disc,
    'disc-3': Disc3,
    monitor: Monitor,
    bell: Bell,
    party: PartyPopper,
    image: Image,
    loader: LoaderCircle,
    grip: GripVertical,
    keyboard: Keyboard,
    palette: Palette,
    info: Info,
    cable: Cable,
    'shield-check': ShieldCheck,
    fx: WandSparkles,
    'volume-up': Volume2,
} as const;

export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
interface Props {
    name: IconName;
    size?: number;
}

let { name, size = 14 }: Props = $props();
const Component = $derived(ICONS[name]);
</script>

<Component {size} strokeWidth={1.75} />
