import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';
import { SidePanelApp } from './SidePanelApp';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SidePanelApp />
  </StrictMode>,
);
