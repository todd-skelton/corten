import { render } from "solid-js/web";
import { App } from "./rows.jsx";

render(() => <App />, document.getElementById("main"));
document.documentElement.dataset.ready = String(performance.now());
