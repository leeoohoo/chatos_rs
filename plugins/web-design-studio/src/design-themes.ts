import type { WebDesignTokens } from './schema.js';

export interface WebDesignThemePreset {
  id: string;
  name: string;
  description: string;
  canvasBackground: string;
  preview: [string, string, string];
  tokens: WebDesignTokens;
}

const systemFont = '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif';
const editorialFont = '"Iowan Old Style", "Palatino Linotype", "Book Antiqua", serif';

export const WEB_DESIGN_THEME_PRESETS: WebDesignThemePreset[] = [
  {
    id: 'apple-light',
    name: 'Apple Light',
    description: '克制、明亮、大留白，适合产品官网与发布页。',
    canvasBackground: '#F5F5F7',
    preview: ['#F5F5F7', '#FFFFFF', '#007AFF'],
    tokens: {
      colors: { primary: '#007AFF', accent: '#34C759', surface: '#FFFFFF', text: '#1D1D1F', muted: '#6E6E73' },
      radii: { small: 8, medium: 14, large: 24 },
      typography: { fontFamily: systemFont, baseFontSize: 16 }
    }
  },
  {
    id: 'midnight-product',
    name: 'Midnight Product',
    description: '深色高对比，适合 AI、开发者工具与专业产品。',
    canvasBackground: '#09090B',
    preview: ['#09090B', '#18181B', '#8B5CF6'],
    tokens: {
      colors: { primary: '#8B5CF6', accent: '#22D3EE', surface: '#18181B', text: '#FAFAFA', muted: '#A1A1AA' },
      radii: { small: 8, medium: 16, large: 28 },
      typography: { fontFamily: systemFont, baseFontSize: 16 }
    }
  },
  {
    id: 'aurora-saas',
    name: 'Aurora SaaS',
    description: '蓝紫渐进的科技感，适合 SaaS 与增长型落地页。',
    canvasBackground: '#F7F8FF',
    preview: ['#F7F8FF', '#FFFFFF', '#635BFF'],
    tokens: {
      colors: { primary: '#635BFF', accent: '#00B8D9', surface: '#FFFFFF', text: '#17152B', muted: '#6F6B80' },
      radii: { small: 10, medium: 18, large: 30 },
      typography: { fontFamily: systemFont, baseFontSize: 16 }
    }
  },
  {
    id: 'warm-editorial',
    name: 'Warm Editorial',
    description: '温暖纸张与衬线气质，适合品牌、内容与作品集。',
    canvasBackground: '#F7F1E8',
    preview: ['#F7F1E8', '#FFFDF8', '#B45309'],
    tokens: {
      colors: { primary: '#B45309', accent: '#0F766E', surface: '#FFFDF8', text: '#29231D', muted: '#786F65' },
      radii: { small: 4, medium: 10, large: 18 },
      typography: { fontFamily: editorialFont, baseFontSize: 17 }
    }
  },
  {
    id: 'mono-pro',
    name: 'Mono Pro',
    description: '黑白秩序与精确边界，适合工作台、数据与 B2B。',
    canvasBackground: '#F4F4F5',
    preview: ['#F4F4F5', '#FFFFFF', '#18181B'],
    tokens: {
      colors: { primary: '#18181B', accent: '#52525B', surface: '#FFFFFF', text: '#18181B', muted: '#71717A' },
      radii: { small: 5, medium: 9, large: 16 },
      typography: { fontFamily: systemFont, baseFontSize: 15 }
    }
  },
  {
    id: 'fresh-commerce',
    name: 'Fresh Commerce',
    description: '清新自然、友好醒目，适合消费品牌与电商页面。',
    canvasBackground: '#F4F8F1',
    preview: ['#F4F8F1', '#FFFFFF', '#16803A'],
    tokens: {
      colors: { primary: '#16803A', accent: '#F59E0B', surface: '#FFFFFF', text: '#17301E', muted: '#66756A' },
      radii: { small: 10, medium: 18, large: 32 },
      typography: { fontFamily: systemFont, baseFontSize: 16 }
    }
  }
];
