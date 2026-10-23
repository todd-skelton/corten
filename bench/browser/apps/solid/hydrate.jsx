// The server-rendered page: Solid hydrates the rows renderToString wrote, from the same data.
import { hydrate } from "solid-js/web";
import { App } from "./rows.jsx";

const initial = JSON.parse(document.getElementById("data").textContent);
hydrate(() => <App initial={initial} />, document.getElementById("main"));
document.documentElement.dataset.ready = String(performance.now());
