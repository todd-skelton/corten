import { Link, createFileRoute } from "@tanstack/react-router";

const items = Array.from({ length: 50 }, (_, index) => index + 1);

export const Route = createFileRoute("/")({
  head: () => ({ meta: [{ title: "Card Market" }] }),
  component: Home,
});

function Home() {
  return (
    <>
      <h1>Popular items</h1>
      <ul className="home-list">
        {items.map((n) => (
          <li key={n}>
            <Link to="/items/$id" params={{ id: `item_${n}` }}>{`Card Name ${n}`}</Link>
          </li>
        ))}
      </ul>
    </>
  );
}
