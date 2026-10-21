/// <reference types="vite/client" />
import { HeadContent, Link, Outlet, Scripts, createRootRoute } from "@tanstack/react-router";
import { useEffect, type ReactNode } from "react";
import styles from "../style.css?url";

export const Route = createRootRoute({
  head: () => ({
    meta: [{ charSet: "utf-8" }, { name: "viewport", content: "width=device-width, initial-scale=1" }, { title: "Card Market" }],
    links: [{ rel: "stylesheet", href: styles }],
  }),
  shellComponent: Document,
  component: Root,
});

function Document({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <head>
        <HeadContent />
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

function Root() {
  // Effects run after hydration commits; the browser benchmark reads this mark.
  useEffect(() => {
    (window as unknown as { __hydratedAt?: number }).__hydratedAt = performance.now();
  }, []);
  return <Outlet />;
}
