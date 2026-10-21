import { useEffect } from "react";
import { Link, Links, Meta, Outlet, Scripts } from "react-router";
import styles from "./style.css?url";

export const links = () => [{ rel: "stylesheet", href: styles }];

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <Meta />
        <Links />
      </head>
      <body>
        <header className="site">
          <Link to="/">Card Market</Link>
        </header>
        <main>{children}</main>
        <Scripts />
      </body>
    </html>
  );
}

export default function App() {
  // Effects run after hydration commits; the browser benchmark reads this mark.
  useEffect(() => {
    (window as unknown as { __hydratedAt?: number }).__hydratedAt = performance.now();
  }, []);
  return <Outlet />;
}
