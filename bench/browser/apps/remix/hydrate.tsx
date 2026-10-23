// The server-rendered page: run() finds the client entry renderToString marked, loads this
// module for it, and hydrates the rows with the props the server serialized.
import { run } from "remix/component";
export { RowsBench } from "./RowsBench.tsx";

const app = run({ loadModule: async (url, name) => (await import(url))[name] });
// Not a top-level await: run() imports this module for the entry, which would wait on itself.
app.ready().then(() => (document.documentElement.dataset.ready = String(performance.now())));
