<script setup lang="ts">
// Mounted after hydration; the browser benchmark reads this mark.
onMounted(() => {
  (window as unknown as { __hydratedAt?: number }).__hydratedAt = performance.now();
});
</script>

<template>
  <header class="site"><NuxtLink to="/">Card Market</NuxtLink></header>
  <main><NuxtPage /></main>
</template>
