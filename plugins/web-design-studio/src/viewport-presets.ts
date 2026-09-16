import type { WebDesignDevice } from './schema.js';

export type WebDesignViewportOrientation = 'default' | 'rotated';

export interface WebDesignViewportPreset {
  id: string;
  device: WebDesignDevice;
  label: string;
  width: number;
  height: number;
  group?: 'large-display';
}

export interface WebDesignViewportDimensions {
  width: number;
  height: number;
}

export interface WebDesignArtboardSizePreset {
  id: string;
  label: string;
  width: number;
  height: number;
  group?: 'large-surface';
}

export const WEB_DESIGN_ARTBOARD_SIZE_PRESETS: WebDesignArtboardSizePreset[] = [
  { id: 'artboard-narrow-360', label: '窄画板 360', width: 360, height: 800 },
  { id: 'artboard-narrow-390', label: '窄画板 390', width: 390, height: 844 },
  { id: 'artboard-medium-768', label: '中型画板 768', width: 768, height: 1024 },
  { id: 'artboard-standard-1200', label: '紧凑画板 1200', width: 1200, height: 900 },
  { id: 'artboard-wide-1440', label: '标准桌面画板 1440', width: 1440, height: 900 },
  { id: 'artboard-wide-1920', label: '宽屏画板 1920', width: 1920, height: 1080 },
  { id: 'artboard-large-2560', label: '2K QHD 画板 2560', width: 2560, height: 1440, group: 'large-surface' }
];

export function matchArtboardSizePreset(
  width: number,
  height: number
): { preset: WebDesignArtboardSizePreset; orientation: WebDesignViewportOrientation } | undefined {
  for (const preset of WEB_DESIGN_ARTBOARD_SIZE_PRESETS) {
    if (preset.width === width && preset.height === height) return { preset, orientation: 'default' };
    if (preset.height === width && preset.width === height) return { preset, orientation: 'rotated' };
  }
  return undefined;
}

export const WEB_DESIGN_VIEWPORT_PRESETS: WebDesignViewportPreset[] = [
  { id: 'desktop-responsive', device: 'desktop', label: '默认桌面 1440', width: 1440, height: 900 },
  { id: 'desktop-compact-1200', device: 'desktop', label: '紧凑桌面 1200', width: 1200, height: 900 },
  { id: 'desktop-hd', device: 'desktop', label: '笔记本 HD', width: 1366, height: 768 },
  { id: 'macbook-air-13', device: 'desktop', label: 'MacBook Air 13″', width: 1470, height: 956 },
  { id: 'macbook-pro-14', device: 'desktop', label: 'MacBook Pro 14″', width: 1512, height: 982 },
  { id: 'desktop-wide', device: 'desktop', label: '宽屏桌面', width: 1728, height: 1117 },
  { id: 'desktop-full-hd', device: 'desktop', label: 'Full HD / 4K@200%', width: 1920, height: 1080 },
  { id: 'desktop-ultrawide', device: 'desktop', label: '超宽屏 UW-FHD', width: 2560, height: 1080, group: 'large-display' },
  { id: 'desktop-qhd', device: 'desktop', label: '2K QHD', width: 2560, height: 1440, group: 'large-display' },
  { id: 'desktop-ultrawide-qhd', device: 'desktop', label: '超宽屏 UW-QHD', width: 3440, height: 1440, group: 'large-display' },
  { id: 'desktop-4k', device: 'desktop', label: '4K UHD', width: 3840, height: 2160, group: 'large-display' },
  { id: 'desktop-dual-qhd', device: 'desktop', label: '双 QHD 超宽屏', width: 5120, height: 1440, group: 'large-display' },
  { id: 'desktop-5k', device: 'desktop', label: '5K Retina', width: 5120, height: 2880, group: 'large-display' },
  { id: 'desktop-6k', device: 'desktop', label: '6K XDR', width: 6016, height: 3384, group: 'large-display' },
  { id: 'desktop-8k', device: 'desktop', label: '8K UHD', width: 7680, height: 4320, group: 'large-display' },
  { id: 'tablet-responsive', device: 'tablet', label: '响应式平板', width: 768, height: 1024 },
  { id: 'ipad-mini', device: 'tablet', label: 'iPad mini', width: 744, height: 1133 },
  { id: 'ipad-10', device: 'tablet', label: 'iPad 10.9″', width: 820, height: 1180 },
  { id: 'ipad-pro-11', device: 'tablet', label: 'iPad Pro 11″', width: 834, height: 1194 },
  { id: 'ipad-pro-13', device: 'tablet', label: 'iPad Pro 13″', width: 1024, height: 1366 },
  { id: 'mobile-responsive', device: 'mobile', label: '响应式手机', width: 390, height: 844 },
  { id: 'iphone-se', device: 'mobile', label: 'iPhone SE', width: 375, height: 667 },
  { id: 'iphone-16', device: 'mobile', label: 'iPhone 16', width: 393, height: 852 },
  { id: 'iphone-16-pro-max', device: 'mobile', label: 'iPhone 16 Pro Max', width: 440, height: 956 },
  { id: 'pixel-9', device: 'mobile', label: 'Pixel 9', width: 412, height: 915 }
];

export function viewportPresetsForDevice(device: WebDesignDevice): WebDesignViewportPreset[] {
  return WEB_DESIGN_VIEWPORT_PRESETS.filter((preset) => preset.device === device);
}

export function viewportDimensions(
  preset: Pick<WebDesignViewportPreset, 'width' | 'height'>,
  orientation: WebDesignViewportOrientation
): WebDesignViewportDimensions {
  return orientation === 'rotated'
    ? { width: preset.height, height: preset.width }
    : { width: preset.width, height: preset.height };
}

export function matchViewportPreset(
  device: WebDesignDevice,
  width: number
): { preset: WebDesignViewportPreset; orientation: WebDesignViewportOrientation } | undefined {
  for (const preset of viewportPresetsForDevice(device)) {
    if (preset.width === width) return { preset, orientation: 'default' };
    if (preset.height === width) return { preset, orientation: 'rotated' };
  }
  return undefined;
}
