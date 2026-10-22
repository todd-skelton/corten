package bench.itempage;

import java.util.List;
import java.util.Locale;

/** Same view model and rules as sveltekit/src/lib/view-model.ts. */
public record ItemView(String id, String title, String subtitle, String description, List<String> badges, List<String> tags,
        List<Category> categories, List<Field> fields, List<String> images, Summary summary, List<Listing> listings,
        List<Offer> offers, List<ContentLine> contents, List<ContentLine> includedIn) {
    public record Category(String name, String slug) {}
    public record Field(String name, String value) {}
    public record Summary(String lowest, long count, long quantity) {}
    public record Listing(String id, String seller, String rating, long reviews, String condition, String shipsFrom, long quantity,
            String price, double priceValue) {}
    public record Offer(String id, String buyer, String condition, long quantity, String price) {}
    public record ContentLine(String id, String itemId, String title, long quantity) {}

    public static String alertMessage(String title, String email, String target) {
        double price;
        try {
            price = Double.parseDouble(target.trim());
        } catch (NumberFormatException e) {
            price = Double.NaN;
        }
        if (!email.contains("@")) return "Enter a valid email address.";
        if (!(price > 0)) return "Enter a target price above zero.";
        return "Alert set: we will email " + email + " when " + title + " drops below $" + String.format(Locale.ROOT, "%.2f", price) + ".";
    }
}
