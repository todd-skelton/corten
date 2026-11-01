// Same view model and rules as leptos/src/model.rs.
export type Listing = { id: string; seller: string; rating: string | null; reviews: number; condition: string; ships_from: string; quantity: number; price: string; price_value: number };
export type Offer = { id: string; buyer: string; condition: string; quantity: number; price: string };
export type ContentLine = { id: string; item_id: string; title: string; quantity: number };
export type ItemView = {
  id: string; title: string; subtitle: string | null; description: string;
  badges: string[]; tags: string[]; categories: { name: string; slug: string }[];
  fields: { name: string; value: string }[]; images: string[];
  summary: { lowest: string | null; count: number; quantity: number } | null;
  listings: Listing[]; offers: Offer[]; contents: ContentLine[]; included_in: ContentLine[];
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Api = any;
const money = (amount: unknown, code: unknown) =>
  amount == null || code == null ? null : code === "USD" ? `$${amount}` : `${amount} ${code}`;

export function toItemView(a: Api): ItemView {
  return {
    id: String(a.catalog_item_id),
    title: a.title ?? "",
    subtitle: a.subtitle ?? null,
    description: a.description ?? "",
    badges: (a.display_badges ?? []).map((b: Api) => String(b.label)),
    tags: (a.tags ?? []).map(String),
    categories: (a.categories ?? []).map((c: Api) => ({ name: String(c.name), slug: String(c.slug) })),
    fields: (a.field_values ?? []).map((f: Api) => ({
      name: String(f.fieldName ?? f.fieldId),
      value: typeof f.value === "string" ? f.value : JSON.stringify(f.value),
    })),
    images: (a.image_urls ?? []).slice(0, 2).map(String),
    summary: a.market_summary
      ? {
          lowest: money(a.market_summary.lowest_price_amount, a.market_summary.lowest_price_currency_code),
          count: Number(a.market_summary.active_listing_count),
          quantity: Number(a.market_summary.total_visible_quantity),
        }
      : null,
    listings: (a.market_listings ?? []).map((l: Api) => ({
      id: String(l.listing_id),
      seller: l.seller_display_name ?? "Seller",
      rating: l.seller_average_rating ?? null,
      reviews: Number(l.seller_review_count ?? 0),
      condition: l.product_summary ?? "",
      ships_from: l.ship_from_code ?? "",
      quantity: Number(l.visible_quantity ?? 0),
      price: money(l.price_amount, l.price_currency_code) ?? "",
      price_value: Number(l.price_amount),
    })),
    offers: (a.offer_demand_matches ?? []).map((o: Api) => ({
      id: String(o.offer_id),
      buyer: o.buyer_display_name ?? "Buyer",
      condition: o.product_summary ?? "",
      quantity: Number(o.quantity_requested ?? 0),
      price: money(o.price_amount, o.price_currency_code) ?? "",
    })),
    contents: (a.contents ?? []).map((c: Api) => ({
      id: String(c.line_id),
      item_id: String(c.contained_catalog_item_id),
      title: c.contained_item?.title ?? String(c.contained_catalog_item_id),
      quantity: Number(c.quantity ?? 1),
    })),
    included_in: (a.included_in ?? []).map((c: Api) => ({
      id: String(c.line_id),
      item_id: String(c.container_catalog_item_id),
      title: c.container_item?.title ?? String(c.container_catalog_item_id),
      quantity: Number(c.quantity ?? 1),
    })),
  };
}

const counts = new Intl.NumberFormat("en-US");

// A count as an English page writes it: digits grouped, and the word singular for one.
export function counted(n: number, one: string, other: string): string {
  return `${counts.format(n)} ${n === 1 ? one : other}`;
}

export function sortListings(listings: Listing[], descending: boolean): Listing[] {
  const sorted = [...listings].sort((a, b) => a.price_value - b.price_value);
  return descending ? sorted.reverse() : sorted;
}

export function alertMessage(title: string, email: string, target: string): string {
  const price = Number(target.trim());
  if (!email.includes("@")) return "Enter a valid email address.";
  if (!(price > 0)) return "Enter a target price above zero.";
  return `Alert set: we will email ${email} when ${title} drops below $${price.toFixed(2)}.`;
}
