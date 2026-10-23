<!-- Svelte 5: the prices and their directions in $state arrays, the total $derived. -->
<script>
  import { SYMBOLS, start, tick } from "../shared/market.js";

  const prices = $state(SYMBOLS.map((_, i) => start(i)));
  const dirs = $state(SYMBOLS.map(() => "up"));
  const total = $derived(prices.reduce((t, p) => t + p, 0));
  const apply = (/** @type {number} */ n) => {
    for (const [i, p] of tick(n)) {
      dirs[i] = p >= prices[i] ? "up" : "down";
      prices[i] = p;
    }
  };
</script>

<div class="controls"><button id="tick20" onclick={() => apply(20)}>Tick 20</button><button id="tick200" onclick={() => apply(200)}>Tick 200</button></div>
<p>Total <span>{total.toFixed(2)}</span></p>
<div class="cards">
  {#each SYMBOLS as symbol, i (symbol)}
    <div class="card"><b>{symbol}</b> <span class={dirs[i]}>{prices[i].toFixed(2)}</span></div>
  {/each}
</div>
