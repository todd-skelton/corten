import adapter from "@sveltejs/adapter-node";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

// adapter-node 6 assumes https unless told the origin; the benchmark serves plain http on 18413,
// and SvelteKit's CSRF check compares form posts against this origin.
export default defineConfig({ plugins: [sveltekit({ adapter: adapter(), paths: { origin: "http://localhost:18413" } })] });
