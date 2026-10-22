using Microsoft.AspNetCore.Mvc;
using Microsoft.AspNetCore.Mvc.RazorPages;

namespace ItemPage.Pages;

public sealed class ItemModel(ItemSource items) : PageModel
{
    public ItemView? Item { get; private set; }
    public bool AllOffers { get; private set; }
    public string Message { get; private set; } = "";

    public async Task<IActionResult> OnGetAsync(string id, string? offers)
    {
        Item = await items.Get(id);
        AllOffers = offers == "all";
        if (Item is null) Response.StatusCode = 404;
        return Page();
    }

    // The antiforgery token is checked before this runs (Razor Pages' default).
    public async Task<IActionResult> OnPostAsync(string id, string? title, string? email, [FromForm(Name = "target_price")] string? targetPrice)
    {
        Item = await items.Get(id);
        if (Item is null)
        {
            Response.StatusCode = 404;
            return Page();
        }
        Message = ItemView.AlertMessage(title ?? "", email ?? "", targetPrice ?? "");
        // htmx swaps only the message; without JavaScript the whole page comes back.
        return Request.Headers.ContainsKey("HX-Request") ? Partial("_AlertMessage", Message) : Page();
    }
}
