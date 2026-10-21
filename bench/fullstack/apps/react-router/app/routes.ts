import { type RouteConfig, index, route } from "@react-router/dev/routes";

export default [index("routes/home.tsx"), route("items/:id", "routes/item.tsx")] satisfies RouteConfig;
