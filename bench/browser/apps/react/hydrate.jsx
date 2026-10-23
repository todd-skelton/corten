// The server-rendered page: React hydrates the rows renderToString wrote, from the same data.
import { hydrateRoot } from "react-dom/client";
import { App } from "./rows.jsx";

const initial = JSON.parse(document.getElementById("data").textContent);
hydrateRoot(document.getElementById("main"), <App initial={initial} />);
