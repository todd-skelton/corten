import { publish } from "@/lib/live";

export async function POST(request: Request) {
  return publish(await request.text());
}
