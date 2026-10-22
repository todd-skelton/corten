import { type RouteConfig, index, route } from "@react-router/dev/routes";

export default [index("routes/home.tsx"), route("items/:id", "routes/item.tsx"), route("api/items/:id", "routes/api-item.ts"),
  route("live/prices", "routes/live-prices.ts"),
  route("live/publish", "routes/live-publish.ts"),
] satisfies RouteConfig;
