package bench.itempage;

import java.io.File;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Function;
import org.springframework.http.HttpStatusCode;
import org.springframework.stereotype.Component;
import org.springframework.web.client.RestClient;
import tools.jackson.databind.JsonNode;
import tools.jackson.databind.ObjectMapper;

/**
 * ITEM_SOURCE=fixture: API responses from FIXTURE_PATH, converted once at startup, so the benchmark
 * measures the web server alone. Otherwise every call fetches the item from the item API.
 */
@Component
public class ItemSource {
    private final ObjectMapper json;
    private final RestClient api;
    private final Map<String, ItemView> fixture;

    public ItemSource(ObjectMapper json) {
        this.json = json;
        this.api = RestClient.create(System.getenv().getOrDefault("API_BASE", "http://localhost:8080"));
        if ("fixture".equals(System.getenv("ITEM_SOURCE"))) {
            var root = json.readTree(new File(System.getenv().getOrDefault("FIXTURE_PATH", "fixtures/items.json")));
            var items = new HashMap<String, ItemView>();
            for (var entry : root.properties()) items.put(entry.getKey(), from(entry.getValue()));
            this.fixture = items;
        } else {
            this.fixture = null;
        }
    }

    public ItemView get(String id) {
        if (fixture != null) return fixture.get(id);
        var body = api.get().uri("/api/discovery/items/{id}", id).exchange((req, res) -> {
            if (res.getStatusCode().isSameCodeAs(HttpStatusCode.valueOf(404))) return null;
            if (res.getStatusCode().isError()) throw new IllegalStateException("item API returned " + res.getStatusCode());
            return json.readTree(res.getBody());
        });
        return body == null ? null : from(body);
    }

    private static boolean missing(JsonNode v) {
        return v == null || v.isNull() || v.isMissingNode();
    }

    private static String text(JsonNode v) {
        return missing(v) ? "" : v.isString() ? v.stringValue() : v.toString();
    }

    private static String optText(JsonNode v) {
        return v != null && v.isString() ? v.stringValue() : null;
    }

    private static long num(JsonNode v, long fallback) {
        return missing(v) ? fallback : v.asLong();
    }

    private static String money(JsonNode amount, JsonNode code) {
        if (missing(amount) || missing(code)) return null;
        return "USD".equals(text(code)) ? "$" + text(amount) : text(amount) + " " + text(code);
    }

    private static <T> List<T> list(JsonNode a, String key, Function<JsonNode, T> f) {
        var out = new ArrayList<T>();
        var v = a.get(key);
        if (v != null && v.isArray()) for (var x : v) out.add(f.apply(x));
        return out;
    }

    private static ItemView.ContentLine line(JsonNode c, String idKey, String itemKey) {
        var itemId = text(c.get(idKey));
        var item = c.get(itemKey);
        var title = item != null && item.get("title") != null && item.get("title").isString() ? item.get("title").stringValue() : itemId;
        return new ItemView.ContentLine(text(c.get("line_id")), itemId, title, num(c.get("quantity"), 1));
    }

    static ItemView from(JsonNode a) {
        var s = a.get("market_summary");
        var images = list(a, "image_urls", ItemSource::text);
        return new ItemView(
                text(a.get("catalog_item_id")), text(a.get("title")), optText(a.get("subtitle")), text(a.get("description")),
                list(a, "display_badges", b -> text(b.get("label"))),
                list(a, "tags", ItemSource::text),
                list(a, "categories", c -> new ItemView.Category(text(c.get("name")), text(c.get("slug")))),
                list(a, "field_values", f -> new ItemView.Field(missing(f.get("fieldName")) ? text(f.get("fieldId")) : text(f.get("fieldName")), text(f.get("value")))),
                images.subList(0, Math.min(2, images.size())),
                missing(s) ? null : new ItemView.Summary(money(s.get("lowest_price_amount"), s.get("lowest_price_currency_code")),
                        num(s.get("active_listing_count"), 0), num(s.get("total_visible_quantity"), 0)),
                list(a, "market_listings", l -> new ItemView.Listing(text(l.get("listing_id")),
                        missing(l.get("seller_display_name")) ? "Seller" : text(l.get("seller_display_name")),
                        optText(l.get("seller_average_rating")), num(l.get("seller_review_count"), 0), text(l.get("product_summary")),
                        text(l.get("ship_from_code")), num(l.get("visible_quantity"), 0),
                        money(l.get("price_amount"), l.get("price_currency_code")) == null ? "" : money(l.get("price_amount"), l.get("price_currency_code")),
                        parse(text(l.get("price_amount"))))),
                list(a, "offer_demand_matches", o -> new ItemView.Offer(text(o.get("offer_id")),
                        missing(o.get("buyer_display_name")) ? "Buyer" : text(o.get("buyer_display_name")), text(o.get("product_summary")),
                        num(o.get("quantity_requested"), 0),
                        money(o.get("price_amount"), o.get("price_currency_code")) == null ? "" : money(o.get("price_amount"), o.get("price_currency_code")))),
                list(a, "contents", c -> line(c, "contained_catalog_item_id", "contained_item")),
                list(a, "included_in", c -> line(c, "container_catalog_item_id", "container_item")));
    }

    private static double parse(String s) {
        try {
            return Double.parseDouble(s);
        } catch (NumberFormatException e) {
            return Double.NaN;
        }
    }
}
