from django.http import Http404, JsonResponse
from django.shortcuts import render
from django.views.decorators.http import require_GET, require_http_methods

from .items import alert_message, fetch_item


@require_GET
def home(request):
    return render(request, "home.html", {"numbers": range(1, 51)})


@require_http_methods(["GET", "POST"])
def item(request, item_id):
    item = fetch_item(item_id)
    if item is None:
        return render(request, "not_found.html", status=404)
    message = ""
    if request.method == "POST":
        p = request.POST
        message = alert_message(p.get("title", ""), p.get("email", ""), p.get("target_price", ""))
        # htmx swaps only the message; without JavaScript the whole page comes back.
        if request.headers.get("HX-Request"):
            return render(request, "alert_message.html", {"message": message})
    all_offers = request.GET.get("offers") == "all"
    context = {
        "item": item,
        "listings": sorted(item["listings"], key=lambda l: l["price_value"]),
        "offers": item["offers"] if all_offers else item["offers"][:5],
        "more_offers": len(item["offers"]) > 5 and not all_offers,
        "message": message,
    }
    return render(request, "item.html", context)


@require_GET
def item_json(request, item_id):
    item = fetch_item(item_id)
    if item is None:
        raise Http404
    return JsonResponse(item)
