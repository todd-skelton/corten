// @ts-check
// Console errors a browser logs for its own bug, not for anything the page did. Each is set aside
// only in the versions known to have it, so a later version is checked afresh.

/**
 * WebKit 27 (Playwright 1.64's WebKit 27.2) sets a style attribute inside the user-agent shadow
 * tree of a `<select>` (and of a few other built-in controls), so a page whose CSP has
 * `style-src 'self'` logs this line for each, though the page sets no style and the control renders
 * the same as without the policy (WebKit bug 327212, fixed in 324283@main; microsoft/playwright#43497).
 * A real inline style or `<style>` element logs the same line in WebKit; Chromium and Firefox,
 * which run the same checks, still catch it.
 */
const webkitShadowStyle = /^Refused to apply a stylesheet because its hash, its nonce, or 'unsafe-inline' does not appear in the style-src directive/;

/**
 * Returns whether a console error is a known bug of this browser rather than the page's.
 * @param {import("@playwright/test").Browser} browser
 * @returns {(text: string) => boolean}
 */
export function browserBug(browser) {
  const webkit27 = browser.browserType().name() === "webkit" && browser.version().startsWith("27.");
  return (text) => webkit27 && webkitShadowStyle.test(text);
}
