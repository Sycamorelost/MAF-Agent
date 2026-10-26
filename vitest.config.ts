import { defineConfig } from "vitest/config";

// 前端单元测试:纯逻辑模块(stores / lib)为主,node 环境即可;
// 涉及 @tauri-apps/api 的模块在测试内用 vi.mock 拦截 invoke。
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
  },
});
