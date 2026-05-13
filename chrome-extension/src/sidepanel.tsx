import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';
import { SidePanelApp } from './SidePanelApp';
import { setWasmBase } from '@rust-rag/llm';

setWasmBase(chrome.runtime.getURL('wasm/'));

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SidePanelApp />
  </StrictMode>,
);
