// Failover benchmark client. Runs in a node container on the feedbench network, with the Docker
// socket mounted so it can stop and restart instances on the same clock it measures with.
//
// CLIENTS subscribers (split over WORKERS threads, so the load generator is not the bottleneck)
// follow feed s1 through the load balancer exactly like EventSource + corten.js: snapshot, then
// /live?since=cursor; on a dropped stream, reconnect after the server's `retry` with
// Last-Event-ID; on an HTTP error, back off and reconnect. The main thread commits WPS market
// writes per second (N listings each) through the load balancer and runs the schedule (default:
// kill a, start a, kill b, start b; kills are SIGKILL). At the end every client must account for
// every committed seq after its first snapshot, in order: as a patch, or inside a newer snapshot.
// Prints one JSON line.
import http from "node:http";
import { monitorEventLoopDelay } from "node:perf_hooks";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

// BASE is an address (an IP, or localhost): the app answers only the host names it is given, and
// any other gets 421.
const base = process.env.BASE ?? "http://10.77.77.30:8080";
const feed = process.env.FEED ?? "s1";
const clients = Number(process.env.CLIENTS ?? 200);
const workers = Math.max(1, Math.min(clients, Number(process.env.WORKERS ?? 4)));
const wps = Number(process.env.WPS ?? 50);
const perWrite = Number(process.env.N ?? 5);
const seconds = Number(process.env.SECONDS ?? 80);
const schedule = JSON.parse(process.env.SCHEDULE ?? '[[20,"kill","fb-a"],[35,"start","fb-a"],[50,"kill","fb-b"],[65,"start","fb-b"]]');
/** Latency samples written this close after a kill count as failover, not steady state. */
const KILL_WINDOW_MS = 5000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const pct = (sorted, p) => (sorted.length ? +sorted[Math.min(sorted.length - 1, Math.floor(p * (sorted.length - 1) + 0.5))].toFixed(1) : null);
const summary = (xs) => {
  const s = Float64Array.from(xs).sort();
  return { n: s.length, p50: pct(s, 0.5), p95: pct(s, 0.95), p99: pct(s, 0.99), max: pct(s, 1) };
};
const loopDelay = monitorEventLoopDelay({ resolution: 10 });
loopDelay.enable();
const loopStats = () => ({ p50: loopDelay.percentile(50) / 1e6, p99: loopDelay.percentile(99) / 1e6, max: loopDelay.max / 1e6 });

if (isMainThread) await main();
else await subscribers(workerData);

// ---------------------------------------------------------------------------------------------
// Worker: a share of the subscribers.

async function subscribers({ t0, from, count }) {
  let stopping = false;
  /** The committed seq every client must reach before it stops reading. */
  let target = Infinity;
  const live = [];
  let finish;
  const finished = new Promise((r) => (finish = r));
  parentPort.on("message", (m) => {
    if (m.type === "target") {
      stopping = true;
      target = m.target;
    } else if (m.type === "stop") {
      target = -1;
      for (const c of live) c.stop.abort();
      finish(m);
    }
  });

  /** One subscriber. */
  async function subscriber(i) {
    // `last` is the seq the client's rows are current at (a snapshot can move it either way);
    // `high` is the highest seq it ever held. A patch above `high` is new (`received`); one at or
    // below it is a replay after a resync (`redelivered`). A snapshot above `high` stands in for
    // the patches it skipped (`coveredBySnapshot`). A patch that does not follow `last` leaves
    // the rows wrong (`missing`).
    const c = {
      i, first: null, last: 0, high: 0, received: 0, redelivered: 0, coveredBySnapshot: 0, duplicates: 0, missing: 0,
      snapshots: 0, syncs: [], reconnects: 0, httpErrors: 0, networkErrors: 0, instances: [], outages: [], cursor: "",
      retry: 3000, lastPatchAt: 0, since: "", gaps: [], log: [], stop: new AbortController(),
      // per new patch: seq, written (listing updated_at), received (both Unix ms)
      seqs: [], written: [], receivedAt: [],
    };
    live.push(c);
    const done = () => stopping && c.last >= target;
    /** The current outage: from the moment the stream dropped to the first new patch after it. */
    let outage = null;
    const dropped = () => (outage ??= { droppedAt: Date.now() - t0, lastPatchAt: c.lastPatchAt - t0, opens: 0, resynced: false });
    const snapshot = async () => {
      for (let attempt = 0; ; attempt += 1) {
        try {
          const res = await fetch(`${base}/feeds/${feed}`);
          if (!res.ok) throw new Error(String(res.status));
          const snap = await res.json();
          c.snapshots += 1;
          if (c.first === null) c.first = c.high = snap.seq;
          if (snap.seq > c.high) {
            c.coveredBySnapshot += snap.seq - c.high;
            c.high = snap.seq;
          }
          c.last = snap.seq;
          c.since = `${snap.epoch}-${snap.seq}`;
          c.cursor = c.since;
          return;
        } catch {
          await sleep(Math.min(5000, 250 * 2 ** attempt));
        }
      }
    };
    await snapshot();
    let backoff = 0;
    while (!done()) {
      let res;
      try {
        // EventSource keeps its first URL and adds Last-Event-ID; the server prefers the header.
        res = await fetch(`${base}/feeds/${feed}/live?since=${encodeURIComponent(c.since)}`, {
          headers: c.cursor !== c.since ? { "last-event-id": c.cursor } : {},
          signal: AbortSignal.any([AbortSignal.timeout((seconds + 60) * 1000), c.stop.signal]),
        });
      } catch {
        if (done() || c.stop.signal.aborted) break;
        c.networkErrors += 1;
        dropped();
        await sleep(c.retry);
        continue;
      }
      if (!res.ok) {
        // EventSource would close for good; corten.js listens again from its cursor with backoff.
        c.httpErrors += 1;
        dropped();
        backoff += 1;
        await sleep(Math.min(30_000, 500 * 2 ** backoff));
        continue;
      }
      const instance = res.headers.get("x-instance");
      c.instances.push(instance);
      c.log.push([Date.now() - t0, "open", instance, c.cursor]);
      if (outage) {
        outage.openedAt ??= Date.now() - t0;
        outage.opens += 1;
      }
      let resync = false;
      try {
        const decoder = new TextDecoder();
        let buffer = "";
        for await (const chunk of res.body) {
          buffer += decoder.decode(chunk, { stream: true });
          let end;
          while ((end = buffer.indexOf("\n\n")) >= 0) {
            const block = buffer.slice(0, end);
            buffer = buffer.slice(end + 2);
            let event = "message";
            let data = "";
            let id = null;
            for (const line of block.split("\n")) {
              if (line.startsWith("retry: ")) c.retry = Number(line.slice(7));
              else if (line.startsWith("event: ")) event = line.slice(7);
              else if (line.startsWith("data: ")) data = line.slice(6);
              else if (line.startsWith("id: ")) id = line.slice(4);
            }
            if (event === "sync") {
              resync = true;
              c.syncs.push([Date.now() - t0, instance, JSON.parse(data).reason]);
              c.log.push([Date.now() - t0, "sync", data]);
            }
            if (event !== "patch") continue;
            const now = Date.now();
            const batch = JSON.parse(data);
            if (batch.seq <= c.last) {
              c.duplicates += 1;
              continue;
            }
            if (batch.seq !== c.last + 1) {
              c.missing += batch.seq - c.last - 1;
              c.gaps.push([c.last, batch.seq, now - t0]);
            }
            backoff = 0;
            c.last = batch.seq;
            c.cursor = id ?? c.cursor;
            if (batch.seq <= c.high) {
              c.redelivered += 1;
              continue;
            }
            c.high = batch.seq;
            c.received += 1;
            if (outage) {
              c.outages.push({ ...outage, resumedAt: now - t0 });
              outage = null;
            }
            c.lastPatchAt = now;
            c.seqs.push(batch.seq);
            c.written.push(Math.max(...batch.changes.map((ch) => ch.value?.updated_at ?? 0)));
            c.receivedAt.push(now);
          }
          if (done()) break;
        }
      } catch (e) {
        // stream cut (the instance died) or stopped
        c.log.push([Date.now() - t0, "cut", String(e?.cause?.code ?? e?.name ?? e)]);
      }
      c.log.push([Date.now() - t0, "closed", c.last]);
      if (done() || c.stop.signal.aborted) break;
      dropped();
      if (resync) {
        outage.resynced = true;
        await snapshot();
        continue;
      }
      c.reconnects += 1;
      await sleep(c.retry);
    }
    return c;
  }

  const subs = [];
  for (let k = 0; k < count; k += 1) {
    subs.push(subscriber(from + k));
    await sleep(10 * workers);
  }
  // Ready once every client holds its first snapshot.
  while (live.length < count || live.some((c) => c.first === null)) await sleep(50);
  parentPort.postMessage({ type: "ready" });
  // Report progress until the main thread says stop, so it can end the drain early.
  const progress = setInterval(() => parentPort.postMessage({ type: "progress", behind: live.filter((c) => c.last < target).length }), 100);
  const { commits, kills } = await finished;
  clearInterval(progress);
  const results = await Promise.all(subs);

  // Latency per new patch: from the start of the write (write -> client) and from its commit
  // (the feed pipeline alone: relay, JetStream, the instance, SSE). Split by kill proximity.
  const commitAt = new Map(commits);
  const nearKill = (at) => kills.some((k) => at - t0 >= k - 100 && at - t0 < k + KILL_WINDOW_MS);
  const lat = { writeSteady: [], writeKill: [], commitSteady: [], commitKill: [] };
  for (const c of results) {
    for (let k = 0; k < c.seqs.length; k += 1) {
      const written = c.written[k];
      const kill = nearKill(written);
      if (written) (kill ? lat.writeKill : lat.writeSteady).push(c.receivedAt[k] - written);
      const committed = commitAt.get(c.seqs[k]);
      if (committed) (kill ? lat.commitKill : lat.commitSteady).push(c.receivedAt[k] - committed);
    }
  }
  const latency = Object.fromEntries(Object.entries(lat).map(([k, v]) => [k, Float64Array.from(v)]));
  const clientsOut = results.map(({ stop, seqs, written, receivedAt, ...c }) => c);
  parentPort.postMessage({ type: "done", clients: clientsOut, latency, loop: loopStats() }, Object.values(latency).map((a) => a.buffer));
}

// ---------------------------------------------------------------------------------------------
// Main: workers, the writer, the schedule, the report.

/** Docker Engine API over the mounted socket: POST /containers/{name}/{action}. */
function docker(action, name) {
  return new Promise((resolve, reject) => {
    const req = http.request({ socketPath: "/var/run/docker.sock", path: `/containers/${name}/${action}`, method: "POST" }, (res) => {
      res.resume();
      res.on("end", () => (res.statusCode < 300 ? resolve() : reject(new Error(`${action} ${name}: ${res.statusCode}`))));
    });
    req.on("error", reject);
    req.end();
  });
}

/**
 * Commits `wps` writes per second through the load balancer; at most 64 in flight. Each write has
 * an idempotency key and is retried with it (up to 3 attempts) when it fails, as a real client
 * would; `replayed` counts retries the server answered from the key because the first attempt had
 * committed after all. Every write must commit exactly once: committed - first == ok.
 */
async function writer(t0, state) {
  const w = { sent: 0, ok: 0, failed: 0, retries: 0, replayed: 0, skipped: 0, failedAt: [], ms: [], timeline: [], commits: [] };
  let inFlight = 0;
  const interval = 1000 / wps;
  let next = Date.now();
  const attempt = async (key) => {
    const res = await fetch(`${base}/api/feeds/${feed}/market?n=${perWrite}`, {
      method: "POST",
      headers: { "idempotency-key": key },
      signal: AbortSignal.timeout(10_000),
    });
    if (!res.ok) throw new Error(String(res.status));
    return res.json();
  };
  while (!state.stopping) {
    next += interval;
    if (inFlight < 64) {
      inFlight += 1;
      w.sent += 1;
      const start = Date.now();
      const key = crypto.randomUUID();
      (async () => {
        for (let k = 0; ; k += 1) {
          try {
            return await attempt(key);
          } catch (e) {
            w.failedAt.push([start - t0, Date.now() - start, e?.name === "TimeoutError" ? "timeout" : String(e?.message ?? e), k]);
            if (k === 2) throw e;
            w.retries += 1;
            await sleep(500);
          }
        }
      })()
        .then((written) => {
          w.ok += 1;
          if (written.replayed) w.replayed += 1;
          w.ms.push(Date.now() - start);
          w.timeline.push([start - t0, Date.now() - start]);
          if (!written.replayed) w.commits.push([written.seq, written.committed_at]);
        })
        .catch(() => (w.failed += 1))
        .finally(() => (inFlight -= 1));
    } else w.skipped += 1;
    await sleep(Math.max(0, next - Date.now()));
  }
  while (inFlight > 0) await sleep(50);
  return w;
}

async function main() {
  const t0 = Date.now();
  const share = Math.ceil(clients / workers);
  const pool = [];
  for (let k = 0; k * share < clients; k += 1) {
    const worker = new Worker(new URL(import.meta.url), { workerData: { t0, from: k * share, count: Math.min(share, clients - k * share) } });
    const box = { worker, ready: false, behind: Infinity };
    box.result = new Promise((resolve, reject) => {
      worker.on("message", (m) => {
        if (m.type === "ready") box.ready = true;
        else if (m.type === "progress") box.behind = m.behind;
        else if (m.type === "done") resolve(m);
      });
      worker.on("error", reject);
    });
    pool.push(box);
  }
  while (pool.some((b) => !b.ready)) await sleep(50);
  const first = await (await fetch(`${base}/api/feeds/${feed}/seq`)).json();
  await sleep(2000);
  const state = { stopping: false };
  const writing = writer(t0, state);
  const started = Date.now();
  const events = [];
  console.error(`started: ${clients} subscribers on ${pool.length} threads, ${wps} writes/s`);
  for (const [at, action, name] of schedule) {
    await sleep(Math.max(0, started + at * 1000 - Date.now()));
    const when = Date.now();
    await docker(action, name);
    events.push({ at: when - t0, action, name });
    console.error(`${(when - started) / 1000}s: ${action} ${name}`);
  }
  await sleep(Math.max(0, started + seconds * 1000 - Date.now()));
  state.stopping = true;
  const writes = await writing;
  // Every write has returned, so the committed sequence is final. Clients read until they reach
  // it; after 20 s the rest count as lost.
  const committed = await (await fetch(`${base}/api/feeds/${feed}/seq`)).json();
  for (const b of pool) {
    b.behind = Infinity;
    b.worker.postMessage({ type: "target", target: committed });
  }
  const drainStart = Date.now();
  while (pool.some((b) => b.behind > 0) && Date.now() - drainStart < 20_000) await sleep(50);
  const drainMs = Date.now() - drainStart;
  const kills = events.filter((e) => e.action === "kill").map((e) => e.at);
  for (const b of pool) b.worker.postMessage({ type: "stop", commits: writes.commits, kills });
  const parts = await Promise.all(pool.map((b) => b.result));
  for (const b of pool) await b.worker.terminate();
  report({ t0, events, first, committed, drainMs, writes, parts });
}

function report({ t0, events, first, committed, drainMs, writes, parts }) {
  const results = parts.flatMap((p) => p.clients).sort((a, b) => a.i - b.i);
  const merged = (key) => parts.flatMap((p) => Array.from(p.latency[key]));
  const outages = results.flatMap((c) => c.outages);
  const lostPerClient = results.map((c) => c.missing + Math.max(0, committed - c.high));
  const lossDetail = results
    .filter((c, i) => lostPerClient[i] > 0)
    .slice(0, 10)
    .map((c) => ({ i: c.i, first: c.first, last: c.last, high: c.high, gaps: c.gaps, log: c.log.slice(-12) }));
  const syncs = results.flatMap((c) => c.syncs);
  const count = (rows, key) => Object.fromEntries(Object.entries(Object.groupBy(rows, key)).map(([k, v]) => [k, v.length]));
  // Writes around each event, per second: how many started and the slowest commit.
  const perSecond = {};
  for (const [at, ms] of writes.timeline) {
    const s = (perSecond[Math.floor(at / 1000)] ??= { n: 0, maxMs: 0 });
    s.n += 1;
    s.maxMs = Math.max(s.maxMs, ms);
  }
  const aroundEvents = events.map((e) => {
    const s = Math.floor(e.at / 1000);
    return { ...e, perSecond: Array.from({ length: 6 }, (_, k) => [s - 1 + k, perSecond[s - 1 + k] ?? { n: 0, maxMs: 0 }]) };
  });
  const since = (key) => outages.filter((o) => o[key] !== undefined).map((o) => o[key] - o.droppedAt);
  const loops = [loopStats(), ...parts.map((p) => p.loop)];
  console.log(
    JSON.stringify({
      clients, workers: parts.length, wps, perWrite, seconds, t0, events, committed, drainMs,
      // If these threads lag, client-side latency includes it.
      loadGenerator: { loopDelayP99Ms: Math.max(...loops.map((l) => l.p99)), loopDelayMaxMs: Math.max(...loops.map((l) => l.max)) },
      writes: {
        sent: writes.sent, ok: writes.ok, failed: writes.failed, retries: writes.retries, replayed: writes.replayed, skipped: writes.skipped,
        // Commits beyond the writes that succeeded: a write committed twice, or committed and then
        // failed every retry. Must be 0.
        extraCommits: committed - first - writes.ok,
        failedAt: writes.failedAt, commitMs: summary(writes.ms), aroundEvents,
      },
      delivery: {
        // Every client must account for every seq in (first, committed]: as a patch or inside a newer snapshot.
        patchesExpected: results.reduce((s, c) => s + (committed - c.first), 0),
        patchesReceived: results.reduce((s, c) => s + c.received, 0),
        coveredBySnapshot: results.reduce((s, c) => s + c.coveredBySnapshot, 0),
        redelivered: results.reduce((s, c) => s + c.redelivered, 0),
        lost: lostPerClient.reduce((a, b) => a + b, 0),
        clientsWithLoss: lostPerClient.filter((n) => n > 0).length,
        outOfOrder: results.reduce((s, c) => s + c.gaps.length, 0),
        duplicatesDropped: results.reduce((s, c) => s + c.duplicates, 0),
        extraSnapshots: results.reduce((s, c) => s + c.snapshots - 1, 0),
        syncs: { n: syncs.length, byInstanceAndReason: count(syncs, (s) => `${s[1]}:${s[2]}`), bySecond: count(syncs, (s) => Math.floor(s[0] / 1000)) },
        reconnects: results.reduce((s, c) => s + c.reconnects, 0),
        httpErrors: results.reduce((s, c) => s + c.httpErrors, 0),
        networkErrors: results.reduce((s, c) => s + c.networkErrors, 0),
        clientsEndingOn: count(results, (c) => c.instances.at(-1)),
        lossDetail,
      },
      latencyMs: {
        // write start -> patch at the client (includes the Postgres commit)
        writeToClient: { steady: summary(merged("writeSteady")), within5sOfKill: summary(merged("writeKill")) },
        // commit returned -> patch at the client (relay, JetStream, instance, SSE)
        commitToClient: { steady: summary(merged("commitSteady")), within5sOfKill: summary(merged("commitKill")) },
      },
      outages: {
        n: outages.length,
        resynced: outages.filter((o) => o.resynced).length,
        // drop -> stream open again (200) on some instance
        reconnectMs: summary(since("openedAt")),
        // drop -> first new patch (also waits for the next commit)
        resumeMs: summary(since("resumedAt")),
        // last patch before the drop -> first new patch after it
        silentMs: summary(outages.map((o) => o.resumedAt - o.lastPatchAt)),
      },
    }),
  );
}
