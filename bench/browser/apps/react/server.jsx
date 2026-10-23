import { renderToString } from "react-dom/server";
import { App } from "./rows.jsx";

export const render = (initial) => ({ head: "", html: renderToString(<App initial={initial} />) });
