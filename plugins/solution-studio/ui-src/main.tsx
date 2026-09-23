import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { ReactFlowProvider } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import './styles.css';
import { SolutionStudioApp } from './studio/SolutionStudioApp';

createRoot(document.getElementById('root')!).render(<StrictMode><ReactFlowProvider><SolutionStudioApp /></ReactFlowProvider></StrictMode>);
