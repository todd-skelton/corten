// @ts-check
import { ready } from "corten";

// The browser benchmark waits for this mark: the first page's scripts are imported.
ready.then(() => {
  /** @type {any} */ (window).__hydratedAt = performance.now();
});
