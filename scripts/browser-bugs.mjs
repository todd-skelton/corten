// @ts-check
// Console errors a browser logs for its own bug, not for anything the page did. Each is set aside
// only in the version known to have it, so a later version is checked afresh.

/**
 * WebKit 27.2 (Playwright 1.64's) sets a style attribute inside the user-agent shadow tree of a
 * `<select>`, so a page whose CSP has `style-src 'self'` logs this line for each, though the page
 * sets no style and the select renders the same as without the policy (WebKit bug 327212, fixed
 * in 324283@main; [microsoft/playwright#43497 WebKit 27.2 reports a CSP "Refused to apply a
 * stylesheet" error for every <select> since 1.64.0](https://github.com/microsoft/playwright/issues/43497)).
 * A real inline style or `<style>` logs the same line, so in that WebKit every page also reports
 * each CSP violation that is not this one (`reportOthers`) as a console error of its own.
 */
const shadowStyle = /^Refused to apply a stylesheet because its hash, its nonce, or 'unsafe-inline' does not appear in the style-src directive/;

/** In the page: a CSP violation other than WebKit's own style on a `<select>` is a console error. */
function reportOthers() {
  document.addEventListener(
    "securitypolicyviolation",
    (e) => {
      const t = e.target;
      const select = e.violatedDirective === "style-src-attr" && t instanceof HTMLSelectElement && !t.hasAttribute("style");
      if (!select) console.error(`CSP violation: ${e.violatedDirective} ${e.blockedURI} on ${t instanceof Element ? t.nodeName : "the document"}`);
    },
    true,
  );
}

/**
 * Returns whether a console error is a known bug of this browser rather than the page's. In
 * WebKit 27.2 it also makes each context (`newPage` makes its own through `newContext`) report
 * every other CSP violation, so setting that line aside hides only the select's.
 * @param {import("@playwright/test").Browser} browser
 * @returns {(text: string) => boolean}
 */
export function browserBug(browser) {
  if (browser.browserType().name() !== "webkit" || browser.version() !== "27.2") return () => false;
  const newContext = browser.newContext.bind(browser);
  browser.newContext = async (...args) => {
    const context = await newContext(...args);
    await context.addInitScript(reportOthers);
    return context;
  };
  return (text) => shadowStyle.test(text);
}
