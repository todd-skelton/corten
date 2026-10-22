using System.Text.Json;

namespace ItemPage;

/// <summary>
/// ITEM_SOURCE=fixture: API responses from FIXTURE_PATH, converted once, so the benchmark measures
/// the web server alone. Otherwise every call fetches the item from the item API.
/// </summary>
public sealed class ItemSource
{
    static readonly JsonSerializerOptions ApiJson = new() { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower, NumberHandling = System.Text.Json.Serialization.JsonNumberHandling.AllowReadingFromString };
    readonly HttpClient http;
    readonly Dictionary<string, ItemView>? fixture;

    public ItemSource(IHttpClientFactory factory)
    {
        http = factory.CreateClient(nameof(ItemSource));
        if (Environment.GetEnvironmentVariable("ITEM_SOURCE") == "fixture")
        {
            using var file = File.OpenRead(Environment.GetEnvironmentVariable("FIXTURE_PATH") ?? "fixtures/items.json");
            var items = JsonSerializer.Deserialize<Dictionary<string, ApiItem>>(file, ApiJson)!;
            fixture = items.ToDictionary(kv => kv.Key, kv => ItemView.From(kv.Value));
        }
    }

    public async Task<ItemView?> Get(string id)
    {
        if (fixture is not null) return fixture.GetValueOrDefault(id);
        using var res = await http.GetAsync($"/api/discovery/items/{Uri.EscapeDataString(id)}");
        if (res.StatusCode == System.Net.HttpStatusCode.NotFound) return null;
        res.EnsureSuccessStatusCode();
        var api = await res.Content.ReadFromJsonAsync<ApiItem>(ApiJson);
        return ItemView.From(api!);
    }
}
