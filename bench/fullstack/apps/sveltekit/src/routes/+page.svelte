<svelte:head><title>Card Market</title></svelte:head>

<h1>Popular items</h1>
<ul class="home-list">
  {#each Array.from({ length: 50 }, (_, i) => i + 1) as n (n)}
    <li><a href={`/items/item_${n}`}>{`Card Name ${n}`}</a></li>
  {/each}
</ul>
