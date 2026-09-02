import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const developmentHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
    host: developmentHost || false,
    ...(developmentHost
      ? { hmr: { protocol: "ws" as const, host: developmentHost, port: 1421 } }
      : {}),
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
