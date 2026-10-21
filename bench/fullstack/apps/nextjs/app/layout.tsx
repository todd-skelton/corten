import Link from "next/link";
import type { ReactNode } from "react";
import { HydrationMark } from "./hydration-mark";
import "./style.css";

export const metadata = { title: "Card Market" };

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>
        <header className="site">
          <Link href="/">Card Market</Link>
        </header>
        <main>{children}</main>
        <HydrationMark />
      </body>
    </html>
  );
}
