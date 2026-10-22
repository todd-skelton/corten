<script setup lang="ts">
import { sortListings, type ItemView } from "~~/shared/view-model";

const route = useRoute();
const { data: item, error } = await useFetch<ItemView>(`/api/items/${route.params.id}`);
if (error.value || !item.value) throw createError({ statusCode: 404, statusMessage: "Not found", fatal: true });
useHead({ title: () => `${item.value?.title} | Card Market` });

const descending = ref(false);
const showAllOffers = ref(false);
const message = ref("");
const listings = computed(() => sortListings(item.value!.listings, descending.value));
const offers = computed(() => (showAllOffers.value ? item.value!.offers : item.value!.offers.slice(0, 5)));

async function submit(event: Event) {
  const form = event.target as HTMLFormElement;
  message.value = (await $fetch<{ message: string }>("/api/alert", { method: "POST", body: new FormData(form) })).message;
}
</script>

<template>
  <article v-if="item" class="item">
    <nav class="crumbs"><a v-for="c in item.categories" :key="c.slug" :href="`/categories/${c.slug}`">{{ c.name }}</a></nav>
    <h1>{{ item.title }}</h1>
    <p v-if="item.subtitle" class="subtitle">{{ item.subtitle }}</p>
    <ul class="badges"><li v-for="(b, i) in item.badges" :key="i">{{ b }}</li></ul>
    <div class="gallery">
      <div v-for="(url, i) in item.images" :key="url" class="img" role="img" :aria-label="`${item.title} image ${i + 1}`" :data-src="url"></div>
    </div>
    <section class="summary">
      <template v-if="item.summary">
        <span><strong>{{ item.summary.lowest ?? "—" }}</strong> lowest</span> <span>{{ `${item.summary.count} listings` }}</span> <span>{{ `${item.summary.quantity} available` }}</span>
      </template>
      <span v-else>No listings yet</span>
    </section>
    <section class="listings">
      <h2>Listings</h2>
      <button type="button" class="sort" @click="descending = !descending">{{ descending ? "Price: high to low" : "Price: low to high" }}</button>
      <table>
        <thead><tr><th>Seller</th><th>Condition</th><th>Ships from</th><th>Qty</th><th>Price</th></tr></thead>
        <tbody>
          <tr v-for="l in listings" :key="l.id">
            <td>{{ l.seller + (l.rating ? ` · ${l.rating} ★ (${l.reviews})` : "") }}</td>
            <td>{{ l.condition }}</td><td>{{ l.ships_from }}</td><td>{{ l.quantity }}</td><td class="price">{{ l.price }}</td>
          </tr>
        </tbody>
      </table>
    </section>
    <section class="offers">
      <h2>Offers</h2>
      <table>
        <thead><tr><th>Buyer</th><th>Condition</th><th>Qty</th><th>Price</th></tr></thead>
        <tbody>
          <tr v-for="o in offers" :key="o.id"><td>{{ o.buyer }}</td><td>{{ o.condition }}</td><td>{{ o.quantity }}</td><td class="price">{{ o.price }}</td></tr>
        </tbody>
      </table>
      <button v-if="item.offers.length > 5 && !showAllOffers" type="button" class="more" @click="showAllOffers = true">{{ `Show all ${item.offers.length} offers` }}</button>
    </section>
    <section class="alert">
      <h2>Price alert</h2>
      <div class="alert-form">
        <form method="post" action="/api/alert" @submit.prevent="submit">
          <input type="hidden" name="item_id" :value="item.id" />
          <input type="hidden" name="title" :value="item.title" />
          <label>Email<input type="email" name="email" required /></label>
          <label>Alert me below<input type="text" name="target_price" inputmode="decimal" required /></label>
          <button type="submit">Set alert</button>
        </form>
      </div>
      <p class="alert-message" aria-live="polite">{{ message }}</p>
    </section>
    <section class="details">
      <h2>Details</h2>
      <p>{{ item.description }}</p>
      <dl><template v-for="(f, i) in item.fields" :key="i"><dt>{{ f.name }}</dt><dd>{{ f.value }}</dd></template></dl>
      <ul class="tags"><li v-for="(t, i) in item.tags" :key="i">{{ t }}</li></ul>
    </section>
    <section v-if="item.contents.length > 0" class="contents">
      <h2>Contents</h2>
      <ul><li v-for="c in item.contents" :key="c.id"><NuxtLink :to="`/items/${c.item_id}`">{{ c.title }}</NuxtLink>{{ ` × ${c.quantity}` }}</li></ul>
    </section>
    <section v-if="item.included_in.length > 0" class="included">
      <h2>Included in</h2>
      <ul><li v-for="c in item.included_in" :key="c.id"><NuxtLink :to="`/items/${c.item_id}`">{{ c.title }}</NuxtLink>{{ ` × ${c.quantity}` }}</li></ul>
    </section>
  </article>
</template>
