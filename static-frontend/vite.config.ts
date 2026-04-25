import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { resolve } from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      '@': resolve(__dirname, './src'),
    },
  },
  server: {
    port: 3001,
    proxy: {
      '/api': {
        target: 'http://localhost:4001',
        changeOrigin: true,
      },
      '/admin': {
        target: 'http://localhost:4001',
        changeOrigin: true,
      },
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        chat: resolve(__dirname, 'chat/index.html'),
        entries: resolve(__dirname, 'entries/index.html'),
      },
      output: {
        manualChunks: (id) => {
          if (id.includes('node_modules')) {
            if (id.includes('react-syntax-highlighter') || id.includes('prismjs')) return 'vendor-syntax';
            if (id.includes('react-markdown') || id.includes('remark') || id.includes('micromark') || id.includes('vfile') || id.includes('unist') || id.includes('unified')) return 'vendor-markdown';
            if (id.includes('lucide-react')) return 'vendor-icons';
            if (id.includes('@radix-ui') || id.includes('cmdk') || id.includes('vaul') || id.includes('react-remove-scroll')) return 'vendor-ui';
            if (id.includes('@xyflow')) return 'vendor-flow';
            if (id.includes('recharts') || id.includes('d3')) return 'vendor-viz';
            if (id.includes('framer-motion')) return 'vendor-animation';
            if (id.includes('react') || id.includes('react-dom') || id.includes('scheduler') || id.includes('swr')) return 'vendor-core';
            if (id.includes('jose') || id.includes('zod') || id.includes('date-fns') || id.includes('clsx') || id.includes('tailwind-merge')) return 'vendor-utils';
          }
        },
      },
    },
  },
})
