"use client";
import { useEffect } from "react";

// Effects run after hydration commits; the browser benchmark reads this mark.
export function HydrationMark() {
  useEffect(() => {
    (window as unknown as { __hydratedAt?: number }).__hydratedAt = performance.now();
  }, []);
  return null;
}
