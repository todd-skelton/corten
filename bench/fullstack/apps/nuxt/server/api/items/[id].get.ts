// The item as JSON for other services, and the page's data: the page's useFetch calls this
// handler in-process while rendering on the server.
export default defineEventHandler(async (event) => {
  const item = await fetchItem(getRouterParam(event, "id") ?? "");
  if (!item) throw createError({ statusCode: 404, statusMessage: "Not found" });
  return item;
});
