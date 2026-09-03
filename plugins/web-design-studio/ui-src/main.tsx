import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { WebDesignStudioApp } from './studio/WebDesignStudioApp';
import './styles.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <WebDesignStudioApp />
  </StrictMode>
);
