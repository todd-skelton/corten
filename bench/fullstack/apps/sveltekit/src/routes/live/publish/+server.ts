import { publish } from "#lib/server/live";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async ({ request }) => publish(await request.text());
