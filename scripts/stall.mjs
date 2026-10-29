// @ts-check
// A browser smoke that stops making progress (a Playwright call that never returns) would otherwise
// hold its CI job until GitHub's six-hour limit (#317). Each smoke's `check` reports here; after
// `minutes` with none, the run says which check it got past last, and what `report` adds, and exits.

/**
 * Starts the watchdog and returns the function each check calls with its name.
 * @param {number} minutes
 * @param {() => string} [report] more to print when it stalls, such as what is still loading
 * @returns {(name: string) => void}
 */
export function stall(minutes, report = () => "") {
  let last = "none yet";
  let at = Date.now();
  const timer = setInterval(() => {
    if (Date.now() - at < minutes * 60_000) return;
    console.log(`\nSTALLED: no check for ${minutes} minutes. The last was: ${last}\n${report()}`);
    process.exit(1);
  }, 10_000);
  timer.unref();
  return (name) => {
    last = name;
    at = Date.now();
  };
}
