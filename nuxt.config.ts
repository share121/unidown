import tailwindcss from "@tailwindcss/vite";
import Aura from "@primeuix/themes/aura";

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: "2025-07-15",
  devtools: { enabled: true },
  modules: [
    "@nuxt/eslint",
    "@nuxt/scripts",
    "@primevue/nuxt-module",
    "@teages/nuxt-legacy",
  ],
  css: ["./app/assets/css/main.css"],
  vite: {
    plugins: [tailwindcss()],
    optimizeDeps: {
      exclude: ["@ffmpeg/ffmpeg"],
    },
  },
  primevue: {
    options: {
      ripple: true,
      theme: {
        preset: Aura,
      },
    },
  },
  postcss: {
    plugins: {
      "postcss-preset-env": {
        stage: 3,
        minimumVendorImplementations: 2,
      },
    },
  },
  legacy: {
    vite: {
      modernPolyfills: true,
    },
  },
});
