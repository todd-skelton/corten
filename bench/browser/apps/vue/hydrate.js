// The server-rendered page: Vue hydrates the rows renderToString wrote, from the same data.
import { createSSRApp } from "vue";
import App from "./App.vue";

const initial = JSON.parse(document.getElementById("data").textContent);
createSSRApp(App, { initial }).mount("#main");
