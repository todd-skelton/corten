<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import "../style.css";

  let { children }: { children: Snippet } = $props();
  // onMount runs after hydration; the browser benchmark reads this mark.
  onMount(() => {
    (window as unknown as { __hydratedAt?: number }).__hydratedAt = performance.now();
  });
</script>

<header class="site"><a href="/">Card Market</a></header>
<main>{@render children()}</main>
