// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import ReactDOM from 'react-dom/client';
import { ConfigProvider, App as AntdApp } from 'antd';
import enUS from 'antd/locale/en_US';
import zhCN from 'antd/locale/zh_CN';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { BrowserRouter } from 'react-router-dom';

import { App } from './App';
import { I18nProvider, useI18n } from './i18n';
import './styles.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});
const routerBasename =
  import.meta.env.BASE_URL && import.meta.env.BASE_URL !== '/'
    ? import.meta.env.BASE_URL.replace(/\/+$/, '')
    : undefined;

function Root() {
  const { language } = useI18n();

  return (
    <ConfigProvider locale={language === 'zh' ? zhCN : enUS}>
      <AntdApp>
        <QueryClientProvider client={queryClient}>
          <BrowserRouter basename={routerBasename}>
            <App />
          </BrowserRouter>
        </QueryClientProvider>
      </AntdApp>
    </ConfigProvider>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <I18nProvider>
      <Root />
    </I18nProvider>
  </React.StrictMode>,
);
