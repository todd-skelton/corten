import { pricesStream } from "@/lib/live";

export const dynamic = "force-dynamic";

export function GET() {
  return pricesStream();
}
