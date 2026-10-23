// The server-rendered page: Svelte hydrates the rows render() wrote, from the same data.
import { hydrate, flushSync } from "svelte";
import App from "./App.svelte";

const initial = JSON.parse(/** @type {Element} */ (document.getElementById("data")).textContent ?? "[]");
hydrate(App, { target: /** @type {Element} */ (document.getElementById("main")), props: { initial } });
flushSync();
document.documentElement.dataset.ready = String(performance.now());
