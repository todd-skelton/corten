import { Link } from "react-router";

export const meta = () => [{ title: "Card Market" }];

const items = Array.from({ length: 50 }, (_, index) => index + 1);

export default function Home() {
  return (
    <>
      <h1>Popular items</h1>
      <ul className="home-list">
        {items.map((n) => (
          <li key={n}>
            <Link to={`/items/item_${n}`}>{`Card Name ${n}`}</Link>
          </li>
        ))}
      </ul>
    </>
  );
}
