// The live benchmark's apps: each framework's own way to push updates to many clients.
// transport: sse (server-sent events), feed (Corten's feed: a snapshot, then SSE from its cursor), phoenix (Phoenix Channels over WebSocket), actioncable
// (Rails Action Cable), pusher (Laravel Reverb, the Pusher protocol). `path` is the stream (or the
// WebSocket endpoint); every app takes `POST /live/publish` with a JSON body and sends it on to
// every client. Images are the full-stack apps' images (bench/fullstack) except corten's.
export const live = {
  corten: { image: "cb-live-corten", port: 18510, transport: "feed", path: "/live/feed", note: "corten::feed: snapshot, then numbered patches over SSE" },
  leptos: { image: "cb-leptos", port: 18511, transport: "sse", path: "/live/prices", note: "axum Sse + tokio broadcast" },
  nextjs: { image: "cb-nextjs", port: 18512, transport: "sse", path: "/live/prices", note: "route handler, ReadableStream" },
  "tanstack-start": { image: "cb-tanstack-start", port: 18513, transport: "sse", path: "/live/prices", note: "server route, ReadableStream" },
  sveltekit: { image: "cb-sveltekit", port: 18514, transport: "sse", path: "/live/prices", note: "+server.ts, ReadableStream" },
  "react-router": { image: "cb-react-router", port: 18515, transport: "sse", path: "/live/prices", note: "resource route, ReadableStream" },
  astro: { image: "cb-astro", port: 18516, transport: "sse", path: "/live/prices", note: "endpoint, ReadableStream" },
  nuxt: { image: "cb-nuxt", port: 18517, transport: "sse", path: "/live/prices", note: "h3 createEventStream" },
  aspnet: { image: "cb-aspnet", port: 18518, transport: "sse", path: "/live/prices", note: "TypedResults.ServerSentEvents" },
  spring: { image: "cb-spring", port: 18519, transport: "sse", path: "/live/prices", note: "SseEmitter on Tomcat" },
  django: {
    image: "cb-django", port: 18520, transport: "sse", path: "/live/prices", note: "async StreamingHttpResponse on uvicorn (ASGI)",
    // Streaming to many clients needs ASGI; one process, so every client shares one publisher.
    cmd: ["uvicorn", "itempage.asgi:application", "--host", "0.0.0.0", "--port", "8080", "--no-access-log"],
  },
  rails: { image: "cb-rails", port: 18521, transport: "actioncable", path: "/cable", note: "Action Cable (async adapter) in Puma" },
  phoenix: { image: "cb-phoenix", port: 18522, transport: "phoenix", path: "/socket/websocket?vsn=2.0.0", note: "Phoenix Channels + PubSub" },
  laravel: {
    image: "cb-laravel", port: 18523, transport: "pusher", path: "/app/bench-key?protocol=7&client=js&version=8.4.0&flash=false", wsPort: 8081,
    note: "Reverb (a second process) fed by Laravel broadcasting",
    cmd: ["sh", "-c", "php artisan reverb:start --host=0.0.0.0 --port=8081 & exec php artisan octane:frankenphp --host=0.0.0.0 --port=8080 --workers=2"],
  },
};
