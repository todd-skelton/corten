import { pricesStream } from "#lib/server/live";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = () => pricesStream();
