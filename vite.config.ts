import { defineConfig } from 'vite';
import { resolve } from 'node:path';
export default defineConfig({
  build: { rollupOptions: { input: { main: resolve('index.html'), addTodo: resolve('add-todo.html'), export: resolve('export.html'), update: resolve('update.html'), manual: resolve('manual.html'), shortcutSettings: resolve('shortcut-settings.html'), cropBackground: resolve('crop-background.html') } } },
  server: {
    port: 1420,
    strictPort: true,
    // Cargo writes and locks executables here during `tauri dev`.
    watch: { ignored: ['**/src-tauri/target/**'] },
  },
  clearScreen: false,
});
