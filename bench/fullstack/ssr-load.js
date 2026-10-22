// Load against one web app. Items 1..500 so the API's cache stays warm and the web server, not
// the API, is what saturates. Uncompressed responses.
// TARGET=page (default) asks for the item page, TARGET=json for the same item as JSON from the
// app's `GET /api/items/{id}` route, so the two measure the same lookup and differ only in what
// the framework writes.
// MODE=closed (VUS users for DURATION), open (RATE a second for DURATION), or iterations
// (exactly ITERATIONS requests from VUS users, for counting instructions per request).
import http from "k6/http";
import { check } from "k6";

const BASE = __ENV.BASE;
const MODE = __ENV.MODE || "closed";
const PATH = (__ENV.TARGET || "page") === "json" ? "/api/items/item_" : "/items/item_";
const scenarios = {
  closed: { executor: "constant-vus", vus: Number(__ENV.VUS || 64), duration: __ENV.DURATION || "60s" },
  open: { executor: "constant-arrival-rate", rate: Number(__ENV.RATE || 100), timeUnit: "1s", duration: __ENV.DURATION || "60s", preAllocatedVUs: 200, maxVUs: 1000 },
  iterations: { executor: "shared-iterations", vus: Number(__ENV.VUS || 4), iterations: Number(__ENV.ITERATIONS || 5000), maxDuration: "10m" },
};
export const options = {
  discardResponseBodies: true,
  summaryTrendStats: ["avg", "p(50)", "p(95)", "p(99)", "max"],
  scenarios: { load: scenarios[MODE] },
};

export default function () {
  const n = 1 + Math.floor(500 * Math.pow(Math.random(), 2));
  const res = http.get(`${BASE}${PATH}${n}`, { headers: { "Accept-Encoding": "identity" } });
  check(res, { "status 200": (r) => r.status === 200 });
}
