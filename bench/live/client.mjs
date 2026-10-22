// The live benchmark's client, run in a container next to the app. CLIENTS subscribers, split over
// WORKERS threads, connect the way the framework's own browser client does (server-sent events,
// Corten's feed, Phoenix Channels, Action Cable or Pusher over WebSocket). Once all are subscribed it waits
// SETTLE seconds (the runner reads memory then), then POSTs RATE messages a second for SECONDS to
// the app's /live/publish. Each message carries its sequence number and send time; every
// subscriber records the delay to each message it gets, and whether any were missing, repeated
// or out of order. Prints progress lines (CONNECTED, PUBLISHING, DONE) and writes OUT.
import http from "node:http";
import { writeFileSync } from "node:fs";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

const env = process.env;
const now = () => performance.timeOrigin + performance.now();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// The published message's fields, in any key order and possibly JSON-escaped inside a string
// (Pusher). The last "seq" is the message's own: Corten's patch has the feed's before it.
const SEQ = /\\?"seq\\?":(\d+)/g;
const SENT = /\\?"sent\\?":([\d.eE+-]+)/;

if (isMainThread) await main();
else await subscribers(workerData);

async function main() {
  const clients = Number(env.CLIENTS ?? 1000);
  const workers = Math.max(1, Math.min(clients, Number(env.WORKERS ?? 4)));
  const rate = Number(env.RATE ?? 10);
  const seconds = Number(env.SECONDS ?? 30);
  const settle = Number(env.SETTLE ?? 10);
  // New connections a second: steady enough that the test is holding clients, not surviving a storm.
  const ramp = Number(env.RAMP ?? 1000);
  const started = now();
  const pool = [];
  for (let w = 0; w < workers; w += 1) {
    const count = Math.floor(clients / workers) + (w < clients % workers ? 1 : 0);
    pool.push(new Worker(new URL(import.meta.url), { workerData: { count, perSecond: ramp / workers, env: { ...env } } }));
  }
  const replies = (type) => Promise.all(pool.map((w) => new Promise((r) => w.once("message", (m) => m.type === type && r(m)))));
  const ready = await Promise.race([replies("connected"), sleep(180_000).then(() => null)]);
  const connected = ready ? ready.reduce((n, m) => n + m.connected, 0) : 0;
  const failed = ready ? ready.reduce((n, m) => n + m.failed, 0) : clients;
  const connectMs = Math.round(now() - started);
  console.log(`CONNECTED ${connected} of ${clients} in ${connectMs} ms`);
  await sleep(settle * 1000);
  console.log("PUBLISHING");
  const total = rate * seconds;
  // An app that cannot keep up stops getting new prices at four times the planned time; what was
  // sent by then is what every client is expected to have.
  const cutoff = seconds * 4000;
  let published = 0;
  let publishErrors = 0;
  const publishMs = [];
  const begin = now();
  // One publish at a time, so the order messages arrive in is the server's doing, not ours. A
  // publish that runs late delays the next one; publishSeconds shows whether the rate held.
  for (let seq = 1; seq <= total && now() - begin < cutoff; seq += 1) {
    published = seq;
    const due = begin + ((seq - 1) * 1000) / rate;
    if (due > now()) await sleep(due - now());
    const sent = now();
    try {
      const r = await fetch(env.PUBLISH, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ seq, sent, price: "$12.34" }), signal: AbortSignal.timeout(10_000) });
      await r.arrayBuffer();
      if (!r.ok) publishErrors += 1;
    } catch {
      publishErrors += 1;
    }
    publishMs.push(now() - sent);
  }
  const publishSeconds = +((now() - begin) / 1000).toFixed(1);
  // Time for prices already accepted to reach every client.
  await sleep(10_000);
  for (const w of pool) w.postMessage({ type: "stop", published });
  const results = await replies("stats");
  const latencies = new Float64Array(results.reduce((n, r) => n + r.latencies.length, 0));
  let at = 0;
  for (const r of results) { latencies.set(r.latencies, at); at += r.latencies.length; }
  latencies.sort();
  const pct = (p) => (latencies.length ? +latencies[Math.min(latencies.length - 1, Math.floor(p * (latencies.length - 1)))].toFixed(1) : null);
  const sum = (k) => results.reduce((n, r) => n + r[k], 0);
  const out = {
    clients, connected, failed, connectMs, rate, seconds, planned: total, published, publishErrors, publishSeconds,
    publishMs: { p50: +[...publishMs].sort((a, b) => a - b)[Math.floor(publishMs.length / 2)].toFixed(1), max: +Math.max(...publishMs).toFixed(1) },
    expected: connected * published, delivered: sum("delivered"), missing: sum("missing"), outOfOrder: sum("outOfOrder"), duplicates: sum("duplicates"),
    dropped: sum("dropped"), resyncs: sum("resyncs"),
    latencyMs: { p50: pct(0.5), p90: pct(0.9), p99: pct(0.99), max: pct(1) },
  };
  writeFileSync(env.OUT, JSON.stringify(out, null, 1));
  console.log("DONE " + JSON.stringify(out));
  process.exit(0);
}

async function subscribers({ count, perSecond, env }) {
  const conns = [];
  let connected = 0;
  let failed = 0;
  let latencies = new Float64Array(1 << 16);
  let n = 0;
  const record = (c, text) => {
    const sent = SENT.exec(text);
    const seqs = sent ? [...text.matchAll(SEQ)] : [];
    if (!seqs.length) return;
    const seq = Number(seqs[seqs.length - 1][1]);
    const ms = now() - Number(sent[1]);
    if (seq <= c.last) { if (c.seen.has(seq)) c.duplicates += 1; else c.outOfOrder += 1; }
    else { if (c.last && seq > c.last + 1) c.gaps += seq - c.last - 1; c.last = seq; }
    c.seen.add(seq);
    c.delivered += 1;
    if (n === latencies.length) { const grown = new Float64Array(n * 2); grown.set(latencies); latencies = grown; }
    latencies[n++] = ms;
  };
  // `ok`: subscribed. Only those count as dropped when they close, or as missing what they did not get.
  const done = (c, ok) => { if (c.settled) return; c.settled = true; c.ok = ok; ok ? (connected += 1) : (failed += 1); };
  const base = new URL(env.BASE);
  const open = {
    sse(c, path = env.STREAM, onEvent = (text) => record(c, text)) {
      const req = http.get({ host: base.hostname, port: base.port, path, agent: false, headers: { accept: "text/event-stream" } }, (res) => {
        if (res.statusCode !== 200) { res.resume(); return done(c, false); }
        done(c, true);
        res.setEncoding("utf8");
        let buf = "";
        res.on("data", (chunk) => {
          buf += chunk;
          let end;
          while ((end = buf.indexOf("\n\n")) >= 0) { onEvent(buf.slice(0, end)); buf = buf.slice(end + 2); }
        });
        res.on("end", () => { if (req.live && c.ok) c.dropped = true; });
      });
      // `live` is false once we close the request ourselves (stop, or a resync).
      req.live = true;
      req.on("error", () => { if (req.live) { done(c, false); if (c.ok) c.dropped = true; } });
      c.close = () => { req.live = false; req.destroy(); };
    },
    // Corten's feed, as subscribe() in corten.js: load the snapshot, follow the live stream from
    // its cursor, and on a `sync` event (the client fell behind) load a new snapshot and resume.
    feed(c) {
      const follow = () => {
        if (c.stopping) return;
        http.get({ host: base.hostname, port: base.port, path: env.STREAM, agent: false }, (res) => {
          let body = "";
          res.setEncoding("utf8");
          res.on("data", (chunk) => { body += chunk; });
          res.on("end", () => {
            if (res.statusCode !== 200) return done(c, false);
            const { epoch, seq } = JSON.parse(body);
            this.sse(c, `${env.STREAM}/live?since=${epoch}-${seq}`, (text) => {
              if (text.startsWith("event: sync")) { c.resyncs += 1; c.close(); follow(); }
              else record(c, text);
            });
          });
        }).on("error", () => done(c, false));
      };
      follow();
    },
    ws(c, url, headers, onOpen, onText) {
      const ws = new WebSocket(url, { headers });
      ws.onopen = () => onOpen(ws);
      ws.onmessage = (e) => onText(ws, String(e.data));
      ws.onerror = () => done(c, false);
      ws.onclose = () => { if (c.settled) c.dropped = c.ok && !c.stopping; else done(c, false); };
      c.close = () => { c.stopping = true; ws.close(); };
    },
    phoenix(c) {
      const url = `ws://${base.host}${env.STREAM}`;
      this.ws(c, url, { origin: "http://localhost" }, (ws) => {
        ws.send(JSON.stringify(["1", "1", "prices", "phx_join", {}]));
        c.timer = setInterval(() => ws.send(JSON.stringify([null, "hb", "phoenix", "heartbeat", {}])), 30_000);
      }, (_, text) => {
        if (text.includes('"phx_reply"') && text.startsWith('["1"')) return done(c, text.includes('"status":"ok"'));
        record(c, text);
      });
    },
    actioncable(c) {
      const identifier = JSON.stringify({ channel: "PricesChannel" });
      this.ws(c, `ws://${base.host}${env.STREAM}`, { origin: env.BASE }, () => {}, (ws, text) => {
        if (text.includes('"type":"welcome"')) return ws.send(JSON.stringify({ command: "subscribe", identifier }));
        if (text.includes('"type":"confirm_subscription"')) return done(c, true);
        if (text.includes('"type":"reject_subscription"')) return done(c, false);
        if (!text.includes('"type":"ping"')) record(c, text);
      });
    },
    pusher(c) {
      this.ws(c, `ws://${base.hostname}:${env.WS_PORT}${env.STREAM}`, {}, () => {}, (ws, text) => {
        if (text.includes("pusher:connection_established")) return ws.send(JSON.stringify({ event: "pusher:subscribe", data: { channel: "prices" } }));
        if (text.includes("pusher_internal:subscription_succeeded")) return done(c, true);
        if (text.includes('"pusher:ping"')) return ws.send(JSON.stringify({ event: "pusher:pong", data: {} }));
        if (text.includes("pusher:error")) return done(c, false);
        record(c, text);
      });
    },
  };
  const t0 = now();
  for (let i = 0; i < count; i += 1) {
    const due = t0 + (i * 1000) / perSecond;
    if (due > now() + 5) await sleep(due - now());
    const c = { last: 0, seen: new Set(), delivered: 0, outOfOrder: 0, duplicates: 0, gaps: 0, resyncs: 0, dropped: false, settled: false };
    conns.push(c);
    open[env.TRANSPORT](c);
  }
  const deadline = now() + 120_000;
  while (connected + failed < count && now() < deadline) await sleep(50);
  parentPort.postMessage({ type: "connected", connected, failed: count - connected });
  parentPort.once("message", ({ published }) => {
    for (const c of conns) { c.stopping = true; clearInterval(c.timer); c.close?.(); }
    let missing = 0;
    for (const c of conns) if (c.ok) missing += Math.max(0, published - c.seen.size);
    parentPort.postMessage({
      type: "stats",
      latencies: latencies.slice(0, n),
      delivered: conns.reduce((s, c) => s + c.delivered, 0),
      missing,
      outOfOrder: conns.reduce((s, c) => s + c.outOfOrder, 0),
      duplicates: conns.reduce((s, c) => s + c.duplicates, 0),
      dropped: conns.filter((c) => c.dropped).length,
      resyncs: conns.reduce((s, c) => s + c.resyncs, 0),
    });
  });
}
