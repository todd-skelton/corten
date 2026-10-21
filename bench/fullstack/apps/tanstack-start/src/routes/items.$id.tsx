import { Link, createFileRoute } from "@tanstack/react-router";
import { Fragment, useState, type FormEvent } from "react";
import { createAlert, getItem } from "../item-api";
import { sortListings, type ItemView } from "../view-model";

export const Route = createFileRoute("/items/$id")({
  loader: ({ params }) => getItem({ data: params.id }),
  head: ({ loaderData }) => ({ meta: [{ title: loaderData ? `${loaderData.title} | Card Market` : "Not found" }] }),
  notFoundComponent: () => <p>Not found</p>,
  component: ItemRoute,
});

function ItemRoute() {
  const item = Route.useLoaderData();
  return <ItemDetail key={item.id} item={item} />;
}

function ItemDetail({ item }: { item: ItemView }) {
  const [descending, setDescending] = useState(false);
  const [showAllOffers, setShowAllOffers] = useState(false);
  const [message, setMessage] = useState("");
  const listings = sortListings(item.listings, descending);
  const offers = showAllOffers ? item.offers : item.offers.slice(0, 5);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const result = await createAlert({ data: new FormData(event.currentTarget) });
    setMessage(result.message);
  }
  return (
    <article className="item">
      <nav className="crumbs">
        {item.categories.map((c) => (
          <a key={c.slug} href={`/categories/${c.slug}`}>{c.name}</a>
        ))}
      </nav>
      <h1>{item.title}</h1>
      {item.subtitle ? <p className="subtitle">{item.subtitle}</p> : null}
      <ul className="badges">
        {item.badges.map((b, i) => (
          <li key={i}>{b}</li>
        ))}
      </ul>
      <div className="gallery">
        {item.images.map((url, i) => (
          <div key={url} className="img" role="img" aria-label={`${item.title} image ${i + 1}`} data-src={url}></div>
        ))}
      </div>
      <section className="summary">
        {item.summary ? (
          <>
            <span><strong>{item.summary.lowest ?? "—"}</strong>{" lowest"}</span>
            <span>{`${item.summary.count} listings`}</span>
            <span>{`${item.summary.quantity} available`}</span>
          </>
        ) : (
          <span>No listings yet</span>
        )}
      </section>
      <section className="listings">
        <h2>Listings</h2>
        <button type="button" className="sort" onClick={() => setDescending((d) => !d)}>
          {descending ? "Price: high to low" : "Price: low to high"}
        </button>
        <table>
          <thead>
            <tr><th>Seller</th><th>Condition</th><th>Ships from</th><th>Qty</th><th>Price</th></tr>
          </thead>
          <tbody>
            {listings.map((l) => (
              <tr key={l.id}>
                <td>{l.seller + (l.rating ? ` · ${l.rating} ★ (${l.reviews})` : "")}</td>
                <td>{l.condition}</td>
                <td>{l.ships_from}</td>
                <td>{l.quantity}</td>
                <td className="price">{l.price}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      <section className="offers">
        <h2>Offers</h2>
        <table>
          <thead>
            <tr><th>Buyer</th><th>Condition</th><th>Qty</th><th>Price</th></tr>
          </thead>
          <tbody>
            {offers.map((o) => (
              <tr key={o.id}>
                <td>{o.buyer}</td>
                <td>{o.condition}</td>
                <td>{o.quantity}</td>
                <td className="price">{o.price}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {item.offers.length > 5 && !showAllOffers ? (
          <button type="button" className="more" onClick={() => setShowAllOffers(true)}>
            {`Show all ${item.offers.length} offers`}
          </button>
        ) : null}
      </section>
      <section className="alert">
        <h2>Price alert</h2>
        <div className="alert-form">
          <form method="post" action={createAlert.url} encType="multipart/form-data" onSubmit={submit}>
            <input type="hidden" name="item_id" value={item.id} />
            <input type="hidden" name="title" value={item.title} />
            <label>Email<input type="email" name="email" required /></label>
            <label>Alert me below<input type="text" name="target_price" inputMode="decimal" required /></label>
            <button type="submit">Set alert</button>
          </form>
        </div>
        <p className="alert-message" aria-live="polite">{message}</p>
      </section>
      <section className="details">
        <h2>Details</h2>
        <p>{item.description}</p>
        <dl>
          {item.fields.map((f, i) => (
            <Fragment key={i}><dt>{f.name}</dt><dd>{f.value}</dd></Fragment>
          ))}
        </dl>
        <ul className="tags">
          {item.tags.map((t, i) => (
            <li key={i}>{t}</li>
          ))}
        </ul>
      </section>
      {item.contents.length > 0 ? <ContentList title="Contents" className="contents" lines={item.contents} /> : null}
      {item.included_in.length > 0 ? <ContentList title="Included in" className="included" lines={item.included_in} /> : null}
    </article>
  );
}

function ContentList({ title, className, lines }: { title: string; className: string; lines: ItemView["contents"] }) {
  return (
    <section className={className}>
      <h2>{title}</h2>
      <ul>
        {lines.map((c) => (
          <li key={c.id}>
            <Link to="/items/$id" params={{ id: c.item_id }}>{c.title}</Link>
            {` × ${c.quantity}`}
          </li>
        ))}
      </ul>
    </section>
  );
}
