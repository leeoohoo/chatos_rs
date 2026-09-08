import { defineComponent, h, type App, type Component } from 'vue';
import { Icon as IconifyIcon } from '@iconify/vue';
import lucideArrowLeft from '@iconify-icons/lucide/arrow-left';
import lucideArrowRight from '@iconify-icons/lucide/arrow-right';
import lucideArrowUpRight from '@iconify-icons/lucide/arrow-up-right';
import lucideBox from '@iconify-icons/lucide/box';
import lucideBug from '@iconify-icons/lucide/bug';
import lucideBugOff from '@iconify-icons/lucide/bug-off';
import lucideCalendar from '@iconify-icons/lucide/calendar';
import lucideCheck from '@iconify-icons/lucide/check';
import lucideChevronDown from '@iconify-icons/lucide/chevron-down';
import lucideFile from '@iconify-icons/lucide/file';
import lucideFolder from '@iconify-icons/lucide/folder';
import lucideFolderOpen from '@iconify-icons/lucide/folder-open';
import lucideMusic from '@iconify-icons/lucide/music';
import lucideSearch from '@iconify-icons/lucide/search';
import lucideSettings from '@iconify-icons/lucide/settings';
import lucideSparkles from '@iconify-icons/lucide/sparkles';
import lucideX from '@iconify-icons/lucide/x';
import heroiconsArrowUpTray from '@iconify-icons/heroicons/arrow-up-tray-20-solid';
import heroiconsBookOpen from '@iconify-icons/heroicons/book-open-solid';
import heroiconsEllipsisVertical from '@iconify-icons/heroicons/ellipsis-vertical';
import tablerArrowLeft from '@iconify-icons/tabler/arrow-narrow-left';
import tablerArrowRight from '@iconify-icons/tabler/arrow-narrow-right';
import tablerX from '@iconify-icons/tabler/x';
import mdiGithub from '@iconify-icons/mdi/github';
import simpleIconsGithub from '@iconify-icons/simple-icons/github';
import simpleIconsOpenai from '@iconify-icons/simple-icons/openai';
import logosGithub from '@iconify-icons/logos/github-icon';
import logosGoogleDrive from '@iconify-icons/logos/google-drive';
import logosGoogleGmail from '@iconify-icons/logos/google-gmail';
import logosMessenger from '@iconify-icons/logos/messenger';
import logosNotion from '@iconify-icons/logos/notion-icon';
import logosWhatsapp from '@iconify-icons/logos/whatsapp-icon';
import deviconGoogleCloud from '@iconify-icons/devicon/googlecloud';

const ClientOnly = defineComponent({
  name: 'ClientOnly',
  setup(_props, { slots }) { return () => slots.default?.(); }
});

const NuxtLink = defineComponent({
  name: 'NuxtLink',
  inheritAttrs: false,
  props: { to: { type: [String, Object], default: '#' } },
  setup(props, { attrs, slots }) {
    return () => h('a', { ...attrs, href: typeof props.to === 'string' ? props.to : '#' }, slots.default?.());
  }
});

const UButton = defineComponent({
  name: 'UButton',
  inheritAttrs: false,
  props: { label: String },
  setup(props, { attrs, slots }) {
    return () => h('button', { ...attrs, type: attrs.type ?? 'button', class: ['runtime-host-button', attrs.class] }, slots.default?.() ?? props.label);
  }
});

const USwitch = defineComponent({
  name: 'USwitch',
  inheritAttrs: false,
  props: { modelValue: Boolean },
  emits: ['update:modelValue'],
  setup(props, { attrs, emit }) {
    return () => h('label', { class: 'runtime-host-switch' }, [
      h('input', { ...attrs, type: 'checkbox', checked: props.modelValue, onChange: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).checked) }),
      h('span')
    ]);
  }
});

const RuntimeIcon = defineComponent({
  name: 'RuntimeIcon',
  inheritAttrs: false,
  props: { name: String },
  setup(props, { attrs }) {
    const icons: Record<string, typeof lucideSparkles> = {
      'lucide:arrow-left': lucideArrowLeft,
      'lucide:arrow-right': lucideArrowRight,
      'lucide:arrow-up-right': lucideArrowUpRight,
      'lucide:box': lucideBox,
      'lucide:bug': lucideBug,
      'lucide:bug-off': lucideBugOff,
      'lucide:calendar': lucideCalendar,
      'lucide:check': lucideCheck,
      'lucide:chevron-down': lucideChevronDown,
      'lucide:file': lucideFile,
      'lucide:folder': lucideFolder,
      'lucide:folder-open': lucideFolderOpen,
      'lucide:music': lucideMusic,
      'lucide:search': lucideSearch,
      'lucide:settings': lucideSettings,
      'lucide:sparkles': lucideSparkles,
      'lucide:x': lucideX,
      'heroicons:arrow-up-tray-20-solid': heroiconsArrowUpTray,
      'heroicons:book-open-solid': heroiconsBookOpen,
      'heroicons:ellipsis-vertical': heroiconsEllipsisVertical,
      'tabler:arrow-narrow-left': tablerArrowLeft,
      'tabler:arrow-narrow-right': tablerArrowRight,
      'tabler:x': tablerX,
      'mdi:github': mdiGithub,
      'simple-icons:github': simpleIconsGithub,
      'simple-icons:openai': simpleIconsOpenai,
      'logos:github-icon': logosGithub,
      'logos:google-drive': logosGoogleDrive,
      'logos:google-gmail': logosGoogleGmail,
      'logos:messenger': logosMessenger,
      'logos:notion-icon': logosNotion,
      'logos:whatsapp-icon': logosWhatsapp,
      'devicon:googlecloud': deviconGoogleCloud
    };
    const normalize = (name = '') => name.startsWith('i-lucide-')
      ? `lucide:${name.slice('i-lucide-'.length)}`
      : name.startsWith('i-simple-icons-')
        ? `simple-icons:${name.slice('i-simple-icons-'.length)}`
        : name;
    return () => {
      const name = normalize(props.name);
      return h(IconifyIcon, {
        ...attrs,
        icon: icons[name] ?? lucideSparkles,
        class: ['runtime-host-icon', attrs.class],
        'aria-label': name || 'icon'
      });
    };
  }
});

const components: Record<string, Component> = {
  ClientOnly,
  NuxtLink,
  UButton,
  USwitch,
  Icon: RuntimeIcon,
  UIcon: RuntimeIcon
};

export function registerVueHostComponents(app: App<Element>) {
  for (const [name, component] of Object.entries(components)) app.component(name, component);
}
