"""Same view model and rules as sveltekit/src/lib/view-model.ts."""
import json
import os
import requests

BASE = os.environ.get("API_BASE", "http://localhost:8080")
_session = requests.Session()


def _money(amount, code):
    if amount is None or code is None:
        return None
    return f"${amount}" if code == "USD" else f"{amount} {code}"


def _line(c, id_key, item_key):
    item_id = str(c[id_key])
    return {
        "id": str(c["line_id"]),
        "item_id": item_id,
        "title": (c.get(item_key) or {}).get("title") or item_id,
        "quantity": int(c.get("quantity") or 1),
    }


def to_item_view(a):
    s = a.get("market_summary")
    return {
        "id": str(a["catalog_item_id"]),
        "title": a.get("title") or "",
        "subtitle": a.get("subtitle"),
        "description": a.get("description") or "",
        "badges": [str(b["label"]) for b in a.get("display_badges") or []],
        "tags": [str(t) for t in a.get("tags") or []],
        "categories": [{"name": str(c["name"]), "slug": str(c["slug"])} for c in a.get("categories") or []],
        "fields": [
            {
                "name": str(f["fieldName"] if f.get("fieldName") is not None else f["fieldId"]),
                "value": f["value"] if isinstance(f["value"], str) else json.dumps(f["value"], separators=(",", ":")),
            }
            for f in a.get("field_values") or []
        ],
        "images": [str(u) for u in (a.get("image_urls") or [])[:2]],
        "summary": {
            "lowest": _money(s.get("lowest_price_amount"), s.get("lowest_price_currency_code")),
            "count": int(s["active_listing_count"]),
            "quantity": int(s["total_visible_quantity"]),
        }
        if s
        else None,
        "listings": [
            {
                "id": str(l["listing_id"]),
                "seller": l.get("seller_display_name") or "Seller",
                "rating": l.get("seller_average_rating"),
                "reviews": int(l.get("seller_review_count") or 0),
                "condition": l.get("product_summary") or "",
                "ships_from": l.get("ship_from_code") or "",
                "quantity": int(l.get("visible_quantity") or 0),
                "price": _money(l.get("price_amount"), l.get("price_currency_code")) or "",
                "price_value": float(l["price_amount"]),
            }
            for l in a.get("market_listings") or []
        ],
        "offers": [
            {
                "id": str(o["offer_id"]),
                "buyer": o.get("buyer_display_name") or "Buyer",
                "condition": o.get("product_summary") or "",
                "quantity": int(o.get("quantity_requested") or 0),
                "price": _money(o.get("price_amount"), o.get("price_currency_code")) or "",
            }
            for o in a.get("offer_demand_matches") or []
        ],
        "contents": [_line(c, "contained_catalog_item_id", "contained_item") for c in a.get("contents") or []],
        "included_in": [_line(c, "container_catalog_item_id", "container_item") for c in a.get("included_in") or []],
    }


# ITEM_SOURCE=fixture: API responses from FIXTURE_PATH, converted once, so the benchmark measures
# the web server alone.
_fixture = None
if os.environ.get("ITEM_SOURCE") == "fixture":
    with open(os.environ.get("FIXTURE_PATH", "fixtures/items.json"), encoding="utf-8") as f:
        _fixture = {k: to_item_view(v) for k, v in json.load(f).items()}


def fetch_item(item_id):
    if _fixture is not None:
        return _fixture.get(item_id)
    res = _session.get(f"{BASE}/api/discovery/items/{requests.utils.quote(item_id, safe='')}")
    if res.status_code == 404:
        return None
    res.raise_for_status()
    return to_item_view(res.json())


def alert_message(title, email, target):
    try:
        price = float(target.strip())
    except ValueError:
        price = float("nan")
    if "@" not in email:
        return "Enter a valid email address."
    if not price > 0:
        return "Enter a target price above zero."
    return f"Alert set: we will email {email} when {title} drops below ${price:.2f}."
