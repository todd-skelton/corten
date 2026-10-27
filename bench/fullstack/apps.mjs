// The apps under test: one image each, same page, same data, same port inside the container.
export const apps = {
  corten: { image: "cb-corten", port: 18410, label: "Corten 0.1 (Rust, axum, html! views)" },
  nextjs: { image: "cb-nextjs", port: 18411, label: "Next.js 16.4 (App Router, RSC)" },
  "tanstack-start": { image: "cb-tanstack-start", port: 18412, label: "TanStack Start 1.169 (React 19, Nitro)" },
  sveltekit: { image: "cb-sveltekit", port: 18413, label: "SvelteKit 3.1 (Svelte 5, adapter-node)" },
  "react-router": { image: "cb-react-router", port: 18414, label: "React Router 7.18 + Express" },
  leptos: { image: "cb-leptos", port: 18415, label: "Leptos 0.8 (Rust, SSR + WASM hydration)" },
  django: { image: "cb-django", port: 18416, label: "Django 6.1 (Python, gunicorn, htmx)" },
  rails: { image: "cb-rails", port: 18417, label: "Rails 8.1 (Ruby, Puma, YJIT, Turbo)" },
  laravel: { image: "cb-laravel", port: 18418, label: "Laravel 13 (PHP, Octane on FrankenPHP, Livewire)" },
  aspnet: { image: "cb-aspnet", port: 18419, label: "ASP.NET Core 10 Razor Pages (C#, Kestrel, htmx)" },
  spring: { image: "cb-spring", port: 18420, label: "Spring Boot 4.1 (Java 25, Thymeleaf, htmx)" },
  phoenix: { image: "cb-phoenix", port: 18421, label: "Phoenix 1.8 LiveView (Elixir)" },
  astro: { image: "cb-astro", port: 18422, label: "Astro 7 (Node adapter, client router)" },
  nuxt: { image: "cb-nuxt", port: 18423, label: "Nuxt 4 (Vue 3, Nitro)" },
};

export const network = process.env.BENCH_NETWORK ?? "rustbench";
/** The item API's container: the Rust API locally, `item-api/` (the fixture over HTTP) on a bench host. */
export const apiContainer = process.env.BENCH_API_CONTAINER ?? "rb-rust";
export const apiBase = `http://${apiContainer}:8080`;
/**
 * CPU limits. By default each app gets one CPU's worth of time (`--cpus 1`). On a dedicated bench
 * host, BENCH_APP_CPUS pins the app to those cores (`--cpuset-cpus`) and BENCH_LOAD_CPUS pins the
 * load generator elsewhere, so the two never share a core.
 */
export const appCpus = process.env.BENCH_APP_CPUS ? ["--cpuset-cpus", process.env.BENCH_APP_CPUS] : ["--cpus", "1"];
export const loadCpus = process.env.BENCH_LOAD_CPUS ? ["--cpuset-cpus", process.env.BENCH_LOAD_CPUS] : ["--cpus", "4"];
/**
 * The load generator's network namespace: every local port, and TIME_WAIT ports reused for new
 * connections. A server that closes each connection (gunicorn's sync workers) would otherwise use
 * up k6's ports within a minute at a few thousand requests a second.
 */
export const k6Net = ["--sysctl", "net.ipv4.ip_local_port_range=1024 65535", "--sysctl", "net.ipv4.tcp_tw_reuse=1"];
/** k6 runs as our user, so it can write its summary into results/ on a Linux host. */
export const k6User = process.getuid ? ["--user", `${process.getuid()}:${process.getgid()}`] : [];

/** `docker run` arguments for one app. `source` is "api" or "fixture". */
export function runArgs(name, app, source, fixturesDir) {
  const args = ["run", "-d", "--name", `fsw-${name}`, "--network", network, ...appCpus, "--memory", "1g", "-p", `${app.port}:8080`];
  // The load generator calls the app by its container name, which Corten answers only when listed.
  args.push("-e", `API_BASE=${apiBase}`, "-e", "PORT=8080", "-e", `HOSTS=fsw-${name}`);
  if (source === "fixture") args.push("-v", `${fixturesDir}:/fixtures:ro`, "-e", "ITEM_SOURCE=fixture", "-e", "FIXTURE_PATH=/fixtures/items.json");
  args.push(app.image);
  return args;
}
