"use client";
import { useActionState, useState } from "react";
import { sortListings, type ItemView } from "@/lib/view-model";
import { createAlert } from "./actions";

export function Listings({ listings }: { listings: ItemView["listings"] }) {
  const [descending, setDescending] = useState(false);
  return (
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
          {sortListings(listings, descending).map((l) => (
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
  );
}

export function Offers({ offers }: { offers: ItemView["offers"] }) {
  const [showAll, setShowAll] = useState(false);
  const shown = showAll ? offers : offers.slice(0, 5);
  return (
    <section className="offers">
      <h2>Offers</h2>
      <table>
        <thead>
          <tr><th>Buyer</th><th>Condition</th><th>Qty</th><th>Price</th></tr>
        </thead>
        <tbody>
          {shown.map((o) => (
            <tr key={o.id}>
              <td>{o.buyer}</td>
              <td>{o.condition}</td>
              <td>{o.quantity}</td>
              <td className="price">{o.price}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {offers.length > 5 && !showAll ? (
        <button type="button" className="more" onClick={() => setShowAll(true)}>
          {`Show all ${offers.length} offers`}
        </button>
      ) : null}
    </section>
  );
}

export function PriceAlert({ id, title }: { id: string; title: string }) {
  const [state, formAction] = useActionState(createAlert, { message: "" });
  return (
    <section className="alert">
      <h2>Price alert</h2>
      <div className="alert-form">
        <form action={formAction}>
          <input type="hidden" name="item_id" value={id} />
          <input type="hidden" name="title" value={title} />
          <label>Email<input type="email" name="email" required /></label>
          <label>Alert me below<input type="text" name="target_price" inputMode="decimal" required /></label>
          <button type="submit">Set alert</button>
        </form>
      </div>
      <p className="alert-message" aria-live="polite">{state.message}</p>
    </section>
  );
}
