export default defineNuxtConfig({
  compatibilityDate: "2026-10-01",
  css: ["~/style.css"],
  devtools: { enabled: false },
  app: { head: { title: "Card Market", meta: [{ name: "viewport", content: "width=device-width, initial-scale=1" }], htmlAttrs: { lang: "en" } } },
});
