import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Configure the React SPA build to match the server's static asset route.
export default defineConfig({
  plugins: [react()],
  base: '/',
  resolve: {
    alias: {
      $lib: '/src/lib'
    }
  },
  build: {
    assetsDir: '_app',
    emptyOutDir: true,
    outDir: 'build'
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:4200',
        rewrite: (path) => path.replace(/^\/api/, '')
      }
    }
  },
  test: {
    environment: 'jsdom',
    environmentOptions: {
      jsdom: {
        url: 'http://localhost:4200'
      }
    },
    globals: true,
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    passWithNoTests: true,
    setupFiles: ['./vitest.setup.ts']
  }
});
