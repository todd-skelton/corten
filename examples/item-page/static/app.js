// @ts-check
import { ready } from "corten";

// The browser benchmark waits for this mark: the first page's islands are defined.
ready.then(() => {
  /** @type {any} */ (window).__hydratedAt = performance.now();
});
