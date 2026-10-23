import { mount, flushSync } from "svelte";
import App from "./App.svelte";

mount(App, { target: /** @type {Element} */ (document.getElementById("main")) });
flushSync();
document.documentElement.dataset.ready = String(performance.now());
