// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import ReleaseAdminPage from './ReleaseAdminPage';
import './styles.css';
import './styles-responsive.css';

const normalizedPath = window.location.pathname.replace(/\/+$/, '') || '/';
const page = normalizedPath === '/admin/releases' ? <ReleaseAdminPage /> : <App />;

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    {page}
  </React.StrictMode>,
);
