import React from 'react';
import ReactDOM from 'react-dom/client';
import { ReactFlowProvider } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import './styles.css';
import { DiagramStudioApp } from './studio/DiagramStudioApp';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ReactFlowProvider>
      <DiagramStudioApp />
    </ReactFlowProvider>
  </React.StrictMode>
);
