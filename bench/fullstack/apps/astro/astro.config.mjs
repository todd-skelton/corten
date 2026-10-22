import node from "@astrojs/node";
import { defineConfig } from "astro/config";

// Every page rendered on request by the Node adapter's own server.
export default defineConfig({
  output: "server",
  adapter: node({ mode: "standalone" }),
});
