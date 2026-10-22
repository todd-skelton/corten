using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Unicode;
using ItemPage;
using Microsoft.Extensions.WebEncoders;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddRazorPages();
// Razor encodes every character outside Basic Latin by default ("·" becomes "&#xB7;"); the usual
// setting for a site with text in other scripts lets them through. HTML's own characters are
// still encoded.
builder.Services.Configure<WebEncoderOptions>(o => o.TextEncoderSettings = new TextEncoderSettings(UnicodeRanges.All));
builder.Services.ConfigureHttpJsonOptions(o => o.SerializerOptions.PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower);
builder.Services.AddHttpClient<ItemSource>(c => c.BaseAddress = new Uri(Environment.GetEnvironmentVariable("API_BASE") ?? "http://localhost:8080"));
builder.Services.AddSingleton<ItemSource>();
builder.Services.AddSingleton<LivePrices>();

var app = builder.Build();
// Load the fixture before the first request.
app.Services.GetRequiredService<ItemSource>();
app.MapStaticAssets();
app.MapRazorPages().WithStaticAssets();
// The item as JSON for other services: the same data the page renders.
app.MapGet("/api/items/{id}", async (string id, ItemSource items) => await items.Get(id) is { } item ? Results.Ok(item) : Results.NotFound());
// The live benchmark (bench/live): server-sent events, and the publisher that feeds them.
app.MapGet("/live/prices", (LivePrices live, CancellationToken cancel) => TypedResults.ServerSentEvents(live.Subscribe(cancel)));
app.MapPost("/live/publish", async (HttpRequest request, LivePrices live) =>
{
    using var reader = new StreamReader(request.Body);
    live.Publish(await reader.ReadToEndAsync());
    return Results.NoContent();
});
app.Run();
