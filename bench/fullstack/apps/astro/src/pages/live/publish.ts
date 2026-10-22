import type { APIRoute } from "astro";
import { publish } from "../../lib/live";

export const POST: APIRoute = async ({ request }) => publish(await request.text());
