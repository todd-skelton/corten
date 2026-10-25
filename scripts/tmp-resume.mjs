const base = "http://localhost:18480";
const read = async (headers, url = "/live/trades", ms = 400) => {
  const ctl = new AbortController(); const t0 = Date.now();
  const res = await fetch(base + url, { headers, signal: ctl.signal });
  const reader = res.body.getReader(); let text = "";
  const timer = setTimeout(() => ctl.abort(), ms);
  try { while (!text.includes("event: patch")) { const { value, done } = await reader.read(); if (done) break; text += new TextDecoder().decode(value); } } catch {}
  clearTimeout(timer);
  const m = text.match(/id: (\d+)/); return { after: Date.now() - t0, id: m?.[1] ?? null };
};
const first = await read({});
console.log("no cursor:", JSON.stringify(first));
const cur = (await read({ "last-event-id": "0" })).id;
console.log("stale cursor 0 ->", cur);
const now = await read({ "last-event-id": cur }, "/live/trades", 300);
console.log("current cursor", cur, "within 300 ms ->", JSON.stringify(now));
