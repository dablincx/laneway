import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// In dev, Vite serves the UI and forwards API + health calls to the laneway server.
const server = process.env.LANEWAY_DEV_SERVER_URL ?? "http://localhost:3000";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  server: {
    host: true,
    port: 5173,
    // locales/ lives next to web/, shared with the server.
    fs: { allow: [".."] },
    proxy: {
      "/api": server,
      "/mcp": server,
      "/healthz": server,
      "/readyz": server,
    },
  },
  build: { outDir: "dist", sourcemap: true },
});
