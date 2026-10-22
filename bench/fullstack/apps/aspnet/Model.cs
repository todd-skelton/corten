using System.Globalization;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ItemPage;

// The item API's response, the fields the page uses.
public sealed record ApiItem(string CatalogItemId, string? Title, string? Subtitle, string? Description,
    List<ApiBadge>? DisplayBadges, List<JsonElement>? Tags, List<ApiCategory>? Categories, List<ApiField>? FieldValues,
    List<string>? ImageUrls, ApiSummary? MarketSummary, List<ApiListing>? MarketListings, List<ApiOffer>? OfferDemandMatches,
    List<ApiLine>? Contents, List<ApiLine>? IncludedIn);
public sealed record ApiBadge(JsonElement Label);
public sealed record ApiCategory(JsonElement Name, JsonElement Slug);
public sealed record ApiField([property: JsonPropertyName("fieldId")] JsonElement FieldId, [property: JsonPropertyName("fieldName")] JsonElement? FieldName, JsonElement Value);
public sealed record ApiSummary(JsonElement? LowestPriceAmount, JsonElement? LowestPriceCurrencyCode, long ActiveListingCount, long TotalVisibleQuantity);
public sealed record ApiListing(JsonElement ListingId, string? SellerDisplayName, string? SellerAverageRating, long? SellerReviewCount,
    string? ProductSummary, string? ShipFromCode, long? VisibleQuantity, JsonElement? PriceAmount, JsonElement? PriceCurrencyCode);
public sealed record ApiOffer(JsonElement OfferId, string? BuyerDisplayName, string? ProductSummary, long? QuantityRequested,
    JsonElement? PriceAmount, JsonElement? PriceCurrencyCode);
public sealed record ApiLine(JsonElement LineId, JsonElement? ContainedCatalogItemId, JsonElement? ContainerCatalogItemId,
    ApiLineItem? ContainedItem, ApiLineItem? ContainerItem, long? Quantity);
public sealed record ApiLineItem(string? Title);

// Same view model and rules as sveltekit/src/lib/view-model.ts.
public sealed record Category(string Name, string Slug);
public sealed record Field(string Name, string Value);
public sealed record Summary(string? Lowest, long Count, long Quantity);
public sealed record Listing(string Id, string Seller, string? Rating, long Reviews, string Condition, string ShipsFrom, long Quantity, string Price, double PriceValue);
public sealed record Offer(string Id, string Buyer, string Condition, long Quantity, string Price);
public sealed record ContentLine(string Id, string ItemId, string Title, long Quantity);

public sealed record ItemView(string Id, string Title, string? Subtitle, string Description, List<string> Badges, List<string> Tags,
    List<Category> Categories, List<Field> Fields, List<string> Images, Summary? Summary, List<Listing> Listings, List<Offer> Offers,
    List<ContentLine> Contents, List<ContentLine> IncludedIn)
{
    static string Text(JsonElement? v) => v is not { } e || e.ValueKind == JsonValueKind.Null ? "" : e.ValueKind == JsonValueKind.String ? e.GetString()! : e.GetRawText();
    static bool Missing(JsonElement? v) => v is not { } e || e.ValueKind == JsonValueKind.Null;
    static string? Money(JsonElement? amount, JsonElement? code) =>
        Missing(amount) || Missing(code) ? null : Text(code) == "USD" ? $"${Text(amount)}" : $"{Text(amount)} {Text(code)}";
    static ContentLine Line(ApiLine c, JsonElement? id, ApiLineItem? item) =>
        new(Text(c.LineId), Text(id), item?.Title ?? Text(id), c.Quantity ?? 1);

    public static ItemView From(ApiItem a) => new(
        a.CatalogItemId, a.Title ?? "", a.Subtitle, a.Description ?? "",
        (a.DisplayBadges ?? []).Select(b => Text(b.Label)).ToList(),
        (a.Tags ?? []).Select(t => Text(t)).ToList(),
        (a.Categories ?? []).Select(c => new Category(Text(c.Name), Text(c.Slug))).ToList(),
        (a.FieldValues ?? []).Select(f => new Field(Missing(f.FieldName) ? Text(f.FieldId) : Text(f.FieldName), Text(f.Value))).ToList(),
        (a.ImageUrls ?? []).Take(2).ToList(),
        a.MarketSummary is { } s ? new Summary(Money(s.LowestPriceAmount, s.LowestPriceCurrencyCode), s.ActiveListingCount, s.TotalVisibleQuantity) : null,
        (a.MarketListings ?? []).Select(l => new Listing(Text(l.ListingId), l.SellerDisplayName ?? "Seller", l.SellerAverageRating, l.SellerReviewCount ?? 0,
            l.ProductSummary ?? "", l.ShipFromCode ?? "", l.VisibleQuantity ?? 0, Money(l.PriceAmount, l.PriceCurrencyCode) ?? "",
            double.TryParse(Text(l.PriceAmount), NumberStyles.Float, CultureInfo.InvariantCulture, out var p) ? p : double.NaN)).ToList(),
        (a.OfferDemandMatches ?? []).Select(o => new Offer(Text(o.OfferId), o.BuyerDisplayName ?? "Buyer", o.ProductSummary ?? "", o.QuantityRequested ?? 0,
            Money(o.PriceAmount, o.PriceCurrencyCode) ?? "")).ToList(),
        (a.Contents ?? []).Select(c => Line(c, c.ContainedCatalogItemId, c.ContainedItem)).ToList(),
        (a.IncludedIn ?? []).Select(c => Line(c, c.ContainerCatalogItemId, c.ContainerItem)).ToList());

    public static string AlertMessage(string title, string email, string target)
    {
        var price = double.TryParse(target.Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var p) ? p : double.NaN;
        if (!email.Contains('@')) return "Enter a valid email address.";
        if (!(price > 0)) return "Enter a target price above zero.";
        return $"Alert set: we will email {email} when {title} drops below ${price.ToString("F2", CultureInfo.InvariantCulture)}.";
    }
}
