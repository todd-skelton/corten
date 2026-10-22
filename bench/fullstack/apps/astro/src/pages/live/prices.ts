import type { APIRoute } from "astro";
import { pricesStream } from "../../lib/live";

export const GET: APIRoute = () => pricesStream();
