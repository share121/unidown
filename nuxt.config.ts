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
        autoprefixer: { grid: true },
      },
    },
  },
  legacy: {
    vite: {
      targets: ["fully supports proxy"],
      modernPolyfills: true,
      additionalLegacyPolyfills: [
        "mdn-polyfills/Element.prototype.getAttributeNames",
      ],
    },
  },
  runtimeConfig: {
    headers: {
      "User-Agent":
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:144.0) Gecko/20100101 Firefox/144.0",
    },
  },
});
