import Link from "next/link";

const items = Array.from({ length: 50 }, (_, index) => index + 1);

export default function Home() {
  return (
    <>
      <h1>Popular items</h1>
      <ul className="home-list">
        {items.map((n) => (
          <li key={n}>
            <Link href={`/items/item_${n}`}>{`Card Name ${n}`}</Link>
          </li>
        ))}
      </ul>
    </>
  );
}
