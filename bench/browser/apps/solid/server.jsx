import { renderToString, generateHydrationScript } from "solid-js/web";
import { App } from "./rows.jsx";

export const render = (initial) => ({ head: generateHydrationScript(), html: renderToString(() => <App initial={initial} />) });
