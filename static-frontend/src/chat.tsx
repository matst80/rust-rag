import React from 'react'
import ReactDOM from 'react-dom/client'
import { Providers } from './components/providers'
import { AppHeader } from './components/app-header'
import { ChatInterface } from './components/chat/chat-interface'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Providers>
      <div className="min-h-screen bg-background text-foreground">
        <AppHeader />
        <main>
          <ChatInterface />
        </main>
      </div>
    </Providers>
  </React.StrictMode>,
)
