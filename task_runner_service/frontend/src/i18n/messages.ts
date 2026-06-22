import { enUS } from './messages/enUS';
import { zhCN } from './messages/zhCN';
import type { MessageDictionary, UiLocale } from './messages/types';

export type { MessageDictionary, UiLocale } from './messages/types';

export const UI_MESSAGES: Record<UiLocale, MessageDictionary> = {
  'zh-CN': zhCN,
  'en-US': enUS,
};
