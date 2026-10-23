<!-- Vue 3: the prices and their directions in reactive arrays, the total computed. -->
<script setup>
import { computed, onMounted, reactive } from "vue";
import { SYMBOLS, start, tick } from "../shared/market.js";

const prices = reactive(SYMBOLS.map((_, i) => start(i)));
const dirs = reactive(SYMBOLS.map(() => "up"));
const total = computed(() => prices.reduce((t, p) => t + p, 0));
const apply = (n) => {
  for (const [i, p] of tick(n)) {
    dirs[i] = p >= prices[i] ? "up" : "down";
    prices[i] = p;
  }
};
onMounted(() => {
  document.documentElement.dataset.ready = String(performance.now());
});
</script>

<template>
  <div class="controls"><button id="tick20" @click="apply(20)">Tick 20</button><button id="tick200" @click="apply(200)">Tick 200</button></div>
  <p>Total <span>{{ total.toFixed(2) }}</span></p>
  <div class="cards">
    <div v-for="(symbol, i) in SYMBOLS" :key="symbol" class="card"><b>{{ symbol }}</b> <span :class="dirs[i]">{{ prices[i].toFixed(2) }}</span></div>
  </div>
</template>
