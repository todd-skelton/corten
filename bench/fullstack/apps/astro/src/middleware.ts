// Imported at startup by the server entry, so a fixture is loaded before the first request.
import "./lib/api";
import { defineMiddleware } from "astro:middleware";

export const onRequest = defineMiddleware((_, next) => next());
