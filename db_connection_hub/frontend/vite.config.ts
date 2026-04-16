import { defineConfig } from "vite";

const backendOrigin = process.env.VITE_DEV_BACKEND_ORIGIN || "http://127.0.0.1:8099";

export default defineConfig({
  server: {
    port: 5174,
    proxy: {
      "/api/v1": {
        target: backendOrigin,
        changeOrigin: true
      }
    }
  }
});
