import { fetchItem } from "@/lib/api";

// The item as JSON for other services: the same data the page renders.
export const dynamic = "force-dynamic";

export async function GET(_: Request, { params }: { params: Promise<{ id: string }> }) {
  const item = await fetchItem((await params).id);
  return item ? Response.json(item) : Response.json({ error: "not found" }, { status: 404 });
}
