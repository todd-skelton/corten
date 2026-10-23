import { createSSRApp } from "vue";
import { renderToString } from "vue/server-renderer";
import App from "./App.vue";

export const render = async (initial) => ({ head: "", html: await renderToString(createSSRApp(App, { initial })) });
