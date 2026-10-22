// Load the fixture (ITEM_SOURCE=fixture) when the server starts, not on the first request.
import "../utils/items";

export default defineNitroPlugin(() => {});
