import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  server: {
    host: '0.0.0.0',
    port: 3000,
    allowedHosts: true,
    hmr: { clientPort: 443 },
    // Артефакты сборки Rust не должны попадать в вотчер (ENOSPC по лимиту inotify)
    watch: { ignored: ['**/src-tauri/target/**'] },
  },
});
