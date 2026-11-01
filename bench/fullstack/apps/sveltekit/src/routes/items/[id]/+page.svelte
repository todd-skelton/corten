<script lang="ts">
  import { enhance } from "$app/forms";
  import { counted, sortListings } from "#lib/view-model";
  import type { PageProps } from "./$types";

  let { data, form }: PageProps = $props();
  let descending = $state(false);
  let showAllOffers = $state(false);
  const item = $derived(data.item);
  const listings = $derived(sortListings(item.listings, descending));
  const offers = $derived(showAllOffers ? item.offers : item.offers.slice(0, 5));
</script>

<svelte:head><title>{`${item.title} | Card Market`}</title></svelte:head>

<article class="item">
  <nav class="crumbs">
    {#each item.categories as c (c.slug)}<a href={`/categories/${c.slug}`}>{c.name}</a>{/each}
  </nav>
  <h1>{item.title}</h1>
  {#if item.subtitle}<p class="subtitle">{item.subtitle}</p>{/if}
  <ul class="badges">
    {#each item.badges as b, i (i)}<li>{b}</li>{/each}
  </ul>
  <div class="gallery">
    {#each item.images as url, i (url)}
      <div class="img" role="img" aria-label={`${item.title} image ${i + 1}`} data-src={url}></div>
    {/each}
  </div>
  <section class="summary">
    {#if item.summary}
      <span><strong>{item.summary.lowest ?? "—"}</strong> lowest</span>
      <span>{counted(item.summary.count, "listing", "listings")}</span>
      <span>{counted(item.summary.quantity, "available", "available")}</span>
    {:else}
      <span>No listings yet</span>
    {/if}
  </section>
  <section class="listings">
    <h2>Listings</h2>
    <button type="button" class="sort" onclick={() => (descending = !descending)}>
      {descending ? "Price: high to low" : "Price: low to high"}
    </button>
    <table>
      <thead><tr><th>Seller</th><th>Condition</th><th>Ships from</th><th>Qty</th><th>Price</th></tr></thead>
      <tbody>
        {#each listings as l (l.id)}
          <tr>
            <td>{l.seller + (l.rating ? ` · ${l.rating} ★ (${l.reviews})` : "")}</td>
            <td>{l.condition}</td>
            <td>{l.ships_from}</td>
            <td>{l.quantity}</td>
            <td class="price">{l.price}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>
  <section class="offers">
    <h2>Offers</h2>
    <table>
      <thead><tr><th>Buyer</th><th>Condition</th><th>Qty</th><th>Price</th></tr></thead>
      <tbody>
        {#each offers as o (o.id)}
          <tr>
            <td>{o.buyer}</td>
            <td>{o.condition}</td>
            <td>{o.quantity}</td>
            <td class="price">{o.price}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if item.offers.length > 5 && !showAllOffers}
      <button type="button" class="more" onclick={() => (showAllOffers = true)}>{`Show all ${item.offers.length} offers`}</button>
    {/if}
  </section>
  <section class="alert">
    <h2>Price alert</h2>
    <div class="alert-form">
      <form method="post" use:enhance={() => ({ update }) => update({ reset: false, invalidateAll: false })}>
        <input type="hidden" name="item_id" value={item.id} />
        <input type="hidden" name="title" value={item.title} />
        <label>Email<input type="email" name="email" required /></label>
        <label>Alert me below<input type="text" name="target_price" inputmode="decimal" required /></label>
        <button type="submit">Set alert</button>
      </form>
    </div>
    <p class="alert-message" aria-live="polite">{form?.message ?? ""}</p>
  </section>
  <section class="details">
    <h2>Details</h2>
    <p>{item.description}</p>
    <dl>
      {#each item.fields as f, i (i)}<dt>{f.name}</dt><dd>{f.value}</dd>{/each}
    </dl>
    <ul class="tags">
      {#each item.tags as t, i (i)}<li>{t}</li>{/each}
    </ul>
  </section>
  {#if item.contents.length > 0}
    <section class="contents">
      <h2>Contents</h2>
      <ul>
        {#each item.contents as c (c.id)}<li><a href={`/items/${c.item_id}`}>{c.title}</a>{` × ${c.quantity}`}</li>{/each}
      </ul>
    </section>
  {/if}
  {#if item.included_in.length > 0}
    <section class="included">
      <h2>Included in</h2>
      <ul>
        {#each item.included_in as c (c.id)}<li><a href={`/items/${c.item_id}`}>{c.title}</a>{` × ${c.quantity}`}</li>{/each}
      </ul>
    </section>
  {/if}
</article>
