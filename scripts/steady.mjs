// @ts-check
// Chromium aborts a navigation in flight with `net::ERR_NETWORK_CHANGED` when the machine's network
// interfaces change, which on a CI runner is Docker adding or removing a bridge while a smoke runs
// (#497). That says nothing about the app, so a smoke's `goto` and `reload` try once more on
// that error alone; any other failure, a second one included, still fails the smoke.

const CHANGED = "net::ERR_NETWORK_CHANGED";

/**
 * Returns `browser`, every page it opens from now on (`newPage`, or `newContext` then
 * `newPage`) retrying `goto` and `reload` once on `ERR_NETWORK_CHANGED`.
 * @template {import("@playwright/test").Browser} B
 * @param {B} browser
 * @returns {B}
 */
export function steady(browser) {
  const newContext = browser.newContext.bind(browser);
  // `browser.newPage` makes its context through `browser.newContext`, so this covers it too.
  browser.newContext = async (...args) => {
    const context = await newContext(...args);
    const newPage = context.newPage.bind(context);
    context.newPage = async () => retrying(await newPage());
    return context;
  };
  return browser;
}

/**
 * @param {import("@playwright/test").Page} page
 * @returns {import("@playwright/test").Page}
 */
function retrying(page) {
  for (const name of /** @type {const} */ (["goto", "reload"])) {
    /** @type {(...args: any[]) => Promise<any>} */
    const call = page[name].bind(page);
    /** @type {any} */ (page)[name] = async (/** @type {any[]} */ ...args) => {
      try {
        return await call(...args);
      } catch (e) {
        if (!(e instanceof Error) || !e.message.includes(CHANGED)) throw e;
        console.log(`retry ${name}: ${CHANGED} (#497)`);
        return call(...args);
      }
    };
  }
  return page;
}
