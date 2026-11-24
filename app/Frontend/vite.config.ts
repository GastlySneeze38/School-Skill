import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri attend l'interface sur ce port exact (Backend/tauri.conf.json > build.devUrl).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // Rust est surveillé par `tauri dev` : inutile que Vite regarde le Backend.
    watch: { ignored: ["**/Backend/**"] },
  },
});
