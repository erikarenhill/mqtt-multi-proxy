import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { readFileSync } from 'fs'

// Read version from Cargo.toml
const cargoToml = readFileSync('../Cargo.toml', 'utf-8')
const versionMatch = cargoToml.match(/^version\s*=\s*"(.+)"/m)
const appVersion = versionMatch ? versionMatch[1] : 'unknown'

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  plugins: [react()],
  server: {
    port: 8080,
    proxy: {
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
      },
    },
  },
})
