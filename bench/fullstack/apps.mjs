// The apps under test: one image each, same page, same data, same port inside the container.
export const apps = {
  corten: { image: "cb-corten", port: 18410, label: "Corten 0.1 (Rust, axum + askama)" },
  nextjs: { image: "cb-nextjs", port: 18411, label: "Next.js 16.4 (App Router, RSC)" },
  "tanstack-start": { image: "cb-tanstack-start", port: 18412, label: "TanStack Start 1.169 (React 19, Nitro)" },
  sveltekit: { image: "cb-sveltekit", port: 18413, label: "SvelteKit 3.1 (Svelte 5, adapter-node)" },
  "react-router": { image: "cb-react-router", port: 18414, label: "React Router 7.18 + Express" },
  leptos: { image: "cb-leptos", port: 18415, label: "Leptos 0.8 (Rust, SSR + WASM hydration)" },
};

export const network = "rustbench";
export const apiBase = "http://rb-rust:8080";

/** `docker run` arguments for one app. `source` is "api" or "fixture". */
export function runArgs(name, app, source, fixturesDir) {
  const args = ["run", "-d", "--name", `fsw-${name}`, "--network", network, "--cpus", "1", "--memory", "1g", "-p", `${app.port}:8080`];
  args.push("-e", `API_BASE=${apiBase}`, "-e", "PORT=8080");
  if (source === "fixture") args.push("-v", `${fixturesDir}:/fixtures:ro`, "-e", "ITEM_SOURCE=fixture", "-e", "FIXTURE_PATH=/fixtures/items.json");
  args.push(app.image);
  return args;
}
