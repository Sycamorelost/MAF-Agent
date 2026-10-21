/// <reference types="node" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri 期望固定端口 + 清晰 HMR
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: 'localhost',
    // 排除 Rust 编译产物目录,避免 Vite watcher 扫描 target 下被锁的 DLL/EXE 导致 EBUSY
    watch: {
      ignored: ['**/src-tauri/target/**', '**/target/**', '**/dist/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'es2022',
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    // elk(1.48MB)/mermaid(673KB) 通过组件内 import() 动态加载,不进首屏;
    // 主包 index 仅 493KB,故放宽告警阈值避免误报。
    chunkSizeWarningLimit: 1500,
    // mermaid/echarts/katex/elk 已通过组件内 import() 动态拆分;
    // 此处把稳定框架与 Markdown 渲染链从主包拆出,减小 index 首屏体积并利于缓存。
    rollupOptions: {
      output: {
        manualChunks: {
          'react-vendor': ['react', 'react-dom', 'react-router-dom'],
          'markdown-vendor': [
            'react-markdown',
            'remark-gfm',
            'remark-math',
            'rehype-katex',
            'rehype-highlight',
            'highlight.js',
            'katex',
          ],
        },
      },
    },
  },
});
