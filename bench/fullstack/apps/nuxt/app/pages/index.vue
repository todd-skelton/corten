<template>
  <h1>Popular items</h1>
  <ul class="home-list">
    <li v-for="n in 50" :key="n"><NuxtLink :to="`/items/item_${n}`">{{ `Card Name ${n}` }}</NuxtLink></li>
  </ul>
</template>
