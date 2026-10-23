# Browser benchmark: the `state()` prototype against plain JavaScript, Solid and Remix 3

A test of one proposal for Corten's browser side. A component's script is a setup function with
plain variables and one `state({...})` object. Assigning a new value to a state key re-runs, in
a microtask, only the renders that read that key: there is no `update()` call, no virtual DOM
and no signal per value. Lists are `<template>` clones that a keyed `list()` patches. The
prototype runtime is [crates/corten/js/state.js](../../crates/corten/js/state.js), 2,147 B gzip
on its own as first measured (2,291 B after the fixes below). Nothing in the framework uses it yet.

Every app renders the same rows (the js-framework-benchmark word lists, the same 32-bit LCG and
the same markup), and the harness measures all of them from outside with the same code.

| App | What it is | Code |
|---|---|---|
| vanilla | Plain JavaScript written for speed, the floor | [apps/vanilla](apps/vanilla/app.js) |
| state | The proposal: `state`, `list`, `mount`, and a row template whose fill function the Rust macro would generate | [apps/state](apps/state/app.js) |
| solid | Solid 1.9.17 with its compiler, written like the js-framework-benchmark's keyed Solid app (a signal per label, `createSelector`) | [apps/solid](apps/solid/rows.jsx) |
| solid-h | Solid through `corten/solid` (`h`, `each`, `mount`), which is what Corten ships today for client-heavy parts | [apps/solid-h](apps/solid-h/app.js) |
| remix | Remix 3.0.0 `remix/component`: plain variables and `handle.update()` (the same component as the rmx bench's) | [apps/remix](apps/remix/RowsBench.tsx) |

Each app is one esbuild bundle, minified (Solid's JSX goes through `babel-preset-solid`).

## Running it

```sh
cd bench/browser
npm install
node build.mjs          # out/<app>/: index.html and app.js, and sizes
node list-check.mjs     # list() against 3,000 random edits in a real DOM
node run.mjs            # client operations, memory and load (two rounds, in alternating order)
node ssr.mjs            # 1,000 server-rendered rows: take over, then interact
node dashboard.mjs      # 200 cards on one shared store
node state-check.mjs    # state() and render scheduling: batching, key switches, abort
node ab.mjs <prep|-> <click> <app> <app> ...   # interleaved means, for sub-millisecond gaps
```

The browser's clock resolves to 0.1 ms here, so differences under a millisecond come from
[ab.mjs](ab.mjs): the apps take turns in blocks of 25 clicks in one browser, and it reports each
app's mean with its standard error.

The harness uses the repository's Playwright and its Chromium.

## Results

The tables in this section are the first run, on commit a755602. [Closing the gaps](#closing-the-gaps)
has what changed after it. They were measured on 2026-10-22 on one Windows desktop, with headless Chromium 148.0.7778.96.
Values are medians in milliseconds. "Commit" runs from the click until the app has updated the
DOM, which every app has done once the microtasks queued during the click have run;
[run.mjs](run.mjs) checks the DOM after every operation, so an app that skips work fails instead
of looking fast. "Frame" runs to the next painted frame. On this machine frames are dominated by
layout and vary by tens of ms between runs, so compare commit times. Two rounds are shown as
`a | b`. Raw data is in [results](results); file names use the UTC date.

### Client operations ([run.mjs](run.mjs))

| Operation (commit ms) | vanilla | state | solid | solid-h | remix |
|---|---|---|---|---|---|
| create 1k rows | 8.1 \| 10.1 | 10.4 \| 11.8 | 12.1 \| 12.0 | 56.1 \| 55.1 | 24.9 \| 37.2 |
| replace 1k | 19.9 \| 20.3 | 18.6 \| 15.8 | 25.0 \| 22.7 | 62.8 \| 70.0 | 47.1 \| 47.6 |
| update every 10th | 0.4 \| 0.4 | 0.5 \| 0.5 | 0.9 \| 0.8 | 0.8 \| 0.8 | 17.2 \| 16.8 |
| select row | 0.2 \| 0.1 | 0.2 \| 0.2 | 0.3 \| 0.3 | 0.3 \| 0.3 | 16.5 \| 17.3 |
| swap rows | 0.2 \| 0.2 | 0.5 \| 0.5 | 1.2 \| 1.1 | 1.6 \| 1.5 | 16.9 \| 17.5 |
| remove row | 0.2 \| 0.2 | 0.4 \| 0.4 | 0.7 \| 0.7 | 1.0 \| 1.1 | 15.6 \| 17.7 |
| create 10k rows | 108 \| 75 | 76 \| 106 | 112 \| 108 | 581 \| 544 | 276 \| 333 |
| append 1k to 1k | 10.2 \| 8.1 | 8.2 \| 10.3 | 11.5 \| 11.4 | 59.0 \| 51.5 | 37.9 \| 51.7 |
| clear 1k | 9.6 \| 9.9 | 10.1 \| 10.2 | 10.9 \| 10.9 | 12.9 \| 11.1 | 9.5 \| 11.8 |
| patch 100 of 1k (live prices) | 0.4 \| 0.5 | 0.5 \| 0.7 | 1.2 \| 1.0 | 0.8 \| 0.8 | 16.0 \| 16.9 |
| patch 1 of 10k | 0.2 \| 0.2 | 0.5 \| 0.5 | 0.1 \| 0.1 | 0.3 \| 0.2 | 141 \| 170 |
| patch 100 of 10k | 0.3 \| 0.5 | 0.7 \| 0.8 | 0.8 \| 0.8 | 1.0 \| 1.0 | 133 \| 135 |
| **JS, gzip** | 1,086 B | 2,927 B | 7,013 B | 9,415 B | 29,120 B |
| **JS heap, 10k rows** | 2,689 KB | 3,456 KB | 10,159 KB | 16,653 KB | 24,867 KB |
| **Heap per row** | 166 B | 243 B | 927 B | 1,584 B | 2,416 B |

### Server-rendered rows ([ssr.mjs](ssr.mjs))

The page arrives with 1,000 rows. Vanilla and state adopt them, while Solid (`renderToString` and
`hydrate`) and Remix (`renderToString` and `run()`) hydrate them. Each figure is the median of
15 fresh loads. No app removed or re-added a row while starting.

| | vanilla | state | solid | remix |
|---|---|---|---|---|
| Take over: parser done to ready | 2.3 | 4.6 | 24.2 | 186.9 |
| JS heap once ready | 1,182 KB | 1,210 KB | 2,235 KB | 3,940 KB |
| HTML, gzip | 14,781 B | 15,257 B | 18,805 B | 14,963 B |
| First select | 0.6 | 1.1 | 1.9 | 26.0 |
| First update every 10th | 0.9 | 1.7 | 1.7 | 22.9 |
| First remove | 0.2 | 0.7 | 1.4 | 21.0 |

### Dashboard ([dashboard.mjs](dashboard.mjs))

The dashboard has 200 cards and a total, all fed by one shared store, and each tick changes 20 or
200 prices. In state, the store is a module-level `state`, each card is its own component whose
render reads one key, and the total's render reads every key. In Solid it is a store with a memo
per card. In Remix, the store is an `EventTarget` and each card calls its own `handle.update()`,
so this is Remix's granular form, not one root update. A mutation count confirmed that a 20-price
tick touches exactly 20 cards and the total in state.

| Commit / frame ms | vanilla | state | solid | remix |
|---|---|---|---|---|
| tick 20 | 0.3 / 14.1 | 0.8 / 14.6 | 0.6 / 14.3 | 4.3 / 14.0 |
| tick 200 | 1.0 / 15.7 | 1.6 / 14.2 | 2.0 / 16.8 | 36.5 / 46.7 |
| JS gzip | 432 B | 1,531 B | 7,328 B | 28,647 B |
| JS heap | 1,140 KB | 1,291 KB | 1,749 KB | 2,060 KB |

## What these numbers do and do not show

1. Implicit tracking with shallow state costs nothing visible against compiled Solid. Every
   operation's commit is within about half a millisecond of Solid's, and most of them are within
   0.3 ms of plain JavaScript.
2. Shallow state has a measurable price on very large lists. Changing one row of 10,000 copies
   the array and compares 10,000 items, which took 0.5 ms here against Solid's 0.1 to 0.2 ms.
   That cost grows linearly. [Closing the gaps](#closing-the-gaps) brought it to within 0.03 ms
   of Solid.
3. Memory is where signals cost the most. A Solid row holds its signals, owner and computations,
   which came to 927 B. A state row is its item and its element, at 243 B, which is about 77 B
   more than plain JavaScript.
4. Taking over server HTML is the largest gap. The state version has no first render, so it reads
   its refs, runs each render once (which finds nothing to change) and listens for events. Solid
   walks hydration keys and builds its graph. Remix re-renders the tree against the DOM, which
   took 187 ms for 1,000 rows, and then pays 16 to 26 ms on each later interaction.
5. Remix's numbers come from its model rather than from a slow implementation. `update()`
   re-renders the component and diffs every row, so its cost tracks the list's size and not the
   change's size. Splitting the work into components with their own updates (the dashboard) helps,
   but each card still re-renders as a whole.
6. Taking over server HTML was first measured at 12.4 ms for state, because `mount` searched all
   2,000 `data-on` attributes to learn the event types. The server knows those types when it
   compiles, so the prototype now reads them from the root's `data-events` attribute, and the
   time fell to 4.6 ms.

## Closing the gaps

The first run found three places where the proposal lost. All three were fixed or explained
without giving back memory or takeover time: bytes per row went from 243 to 241, and takeover
stayed at 4.2 to 4.5 ms. Every figure below was measured with [ab.mjs](ab.mjs) (mean ms ±
standard error) unless marked.

1. **A change to one row of a long list cost the list's length.** `list.set()` called `key()` on
   every item and built a new element array on every change. It now skips an item that is the
   same object as before, and keeps the element array when no item moved. `node(key)` remembers
   the element it found last, so the selected row is not searched for on every render.

   | commit ms | vanilla | state, first run | state now | solid |
   |---|---|---|---|---|
   | patch 1 of 10k | 0.156 ± 0.014 | 0.380 ± 0.018 | 0.279 ± 0.008 | 0.254 ± 0.012 |
   | patch 100 of 10k | 0.776 ± 0.044 | 1.152 ± 0.049 | 0.989 ± 0.055 | 1.521 ± 0.101 |

   The rest of the gap to Solid on one row (about 0.03 ms) is copying the 10,000-item array, which
   is the price of shallow state. Solid pays more at 100 rows because each changed label is its
   own signal update.
2. **The dashboard tick ran behind Solid.** Profiling showed that most of the state version's time
   was the total's `for (const symbol in prices)`. A `for..in` over a `Proxy` takes the
   engine's slow path: an `ownKeys` trap, then a descriptor and a `get` for every key. Reading a
   known list of keys instead (`for (const symbol of SYMBOLS)`) is the fix. Separately, renders
   now keep their keys between runs and only drop the ones they stopped reading, which saves a
   delete and an add per key on every run. [state-check.mjs](state-check.mjs) covers this,
   including a render that switches keys.

   | commit ms | vanilla | state | state with `for..in` | solid | remix |
   |---|---|---|---|---|---|
   | tick 20 | 0.189 ± 0.003 | 0.369 ± 0.011 | 0.578 ± 0.012 | 0.481 ± 0.008 | 3.937 ± 0.043 |
   | tick 200 | 0.860 ± 0.025 | 1.430 ± 0.026 | 1.673 ± 0.031 | 1.800 ± 0.062 | 33.45 ± 0.68 |

   The rule this implies is that a state object holds named fields. A render that needs
   every key should iterate a list it already has, and `html!`'s generated types can give it
   that list. Development mode could warn when a render iterates a state.
3. **Taking over server HTML depended on `data-events`.** This is not a speed problem: with the
   attribute, takeover is 4.2 to 4.5 ms (median of 15 loads), against 2.3 ms for plain
   JavaScript and 22.6 ms for Solid. The open question was whether the server can write the
   attribute. It can: `html!` already finds every `on:event` in a component and already puts
   `data-scope` on the component's one root, so the union of the event types is known when the
   component compiles. That macro change is part of the port and has not been written yet.
   Reading the server's rows lazily was also tried and dropped: takeover did not change (4.2 ms),
   and the first click got slower (2.2 ms against 1.1).

Everything else stayed the same when [run.mjs](run.mjs) was rerun after the fixes (one round,
[results/ops-fixes.json](results/ops-fixes.json)). [list-check.mjs](list-check.mjs) now also
starts lists from server-rendered elements and checks `item()` and `node()` after every edit.
It passed 3,000 random edits.

Known limits of this test:

1. The row template's fill function and `data-events` are hand-written here. In the proposal,
   `html!` generates both, so the generated code is not yet proven.
2. Development mode (frozen state, and a warning when a render writes state) was off for every
   measurement.
3. Every number comes from one machine and one browser. Firefox and Safari were not run.
4. The Remix figures are for an esbuild bundle, which is smaller than what Remix's own asset
   server sends (42.5 KB gzip in 41 files in the rmx bench).
