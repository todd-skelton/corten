import Link from "next/link";
import { notFound } from "next/navigation";
import { Fragment, cache } from "react";
import { fetchItem } from "@/lib/api";
import { counted, type ItemView } from "@/lib/view-model";
import { Listings, Offers, PriceAlert } from "./interactive";

// Every request renders fresh data, like the other stacks.
export const dynamic = "force-dynamic";

const load = cache(fetchItem);

type Props = { params: Promise<{ id: string }> };

export async function generateMetadata({ params }: Props) {
  const item = await load((await params).id);
  return { title: item ? `${item.title} | Card Market` : "Not found" };
}

export default async function ItemPage({ params }: Props) {
  const item = await load((await params).id);
  if (!item) notFound();
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
            <span>{counted(item.summary.count, "listing", "listings")}</span>
            <span>{counted(item.summary.quantity, "available", "available")}</span>
          </>
        ) : (
          <span>No listings yet</span>
        )}
      </section>
      <Listings listings={item.listings} />
      <Offers offers={item.offers} />
      <PriceAlert id={item.id} title={item.title} />
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
            <Link href={`/items/${c.item_id}`}>{c.title}</Link>
            {` × ${c.quantity}`}
          </li>
        ))}
      </ul>
    </section>
  );
}
