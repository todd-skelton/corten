// SSR load against one web app. Items 1..500 so the API's Redis cache stays warm and the
// web server, not the API or Postgres, is what saturates. Uncompressed responses.
import http from "k6/http";
import { check } from "k6";

const BASE = __ENV.BASE;
const MODE = __ENV.MODE || "closed";
export const options = {
  discardResponseBodies: true,
  summaryTrendStats: ["avg", "p(50)", "p(95)", "p(99)", "max"],
  scenarios: MODE === "closed"
    ? { load: { executor: "constant-vus", vus: Number(__ENV.VUS || 64), duration: __ENV.DURATION || "60s" } }
    : { load: { executor: "constant-arrival-rate", rate: Number(__ENV.RATE || 100), timeUnit: "1s", duration: __ENV.DURATION || "60s", preAllocatedVUs: 200, maxVUs: 1000 } },
};

export default function () {
  const n = 1 + Math.floor(500 * Math.pow(Math.random(), 2));
  const res = http.get(`${BASE}/items/item_${n}`, { headers: { "Accept-Encoding": "identity" } });
  check(res, { "status 200": (r) => r.status === 200 });
}
