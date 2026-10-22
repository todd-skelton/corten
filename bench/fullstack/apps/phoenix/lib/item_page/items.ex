defmodule ItemPage.Items do
  @moduledoc """
  The item source and the view model, with the same rules as sveltekit/src/lib/view-model.ts.

  ITEM_SOURCE=fixture: API responses from FIXTURE_PATH, converted once at startup into
  :persistent_term, so the benchmark measures the web server alone. Otherwise every call fetches
  the item from the item API (Req, over its connection pool).
  """

  def load_fixture do
    if System.get_env("ITEM_SOURCE") == "fixture" do
      items =
        System.get_env("FIXTURE_PATH", "fixtures/items.json")
        |> File.read!()
        |> Jason.decode!()
        |> Map.new(fn {id, a} -> {id, from_api(a)} end)

      :persistent_term.put(__MODULE__, items)
    end

    :ok
  end

  def get(id) do
    case :persistent_term.get(__MODULE__, nil) do
      nil -> fetch(id)
      items -> Map.get(items, id)
    end
  end

  defp fetch(id) do
    base = System.get_env("API_BASE", "http://localhost:8080")

    case Req.get!(base <> "/api/discovery/items/" <> URI.encode_www_form(id), retry: false) do
      %{status: 404} -> nil
      %{status: 200, body: body} -> from_api(body)
      %{status: status} -> raise "item API returned #{status}"
    end
  end

  defp text(nil), do: ""
  defp text(v) when is_binary(v), do: v
  defp text(v), do: Jason.encode!(v)

  defp int(nil, default), do: default
  defp int(v, _) when is_integer(v), do: v
  defp int(v, default) when is_binary(v), do: (case Integer.parse(v) do {n, _} -> n; :error -> default end)

  defp money(nil, _), do: nil
  defp money(_, nil), do: nil
  defp money(amount, "USD"), do: "$" <> text(amount)
  defp money(amount, code), do: text(amount) <> " " <> text(code)

  defp number(v) do
    case Float.parse(text(v)) do
      {f, ""} -> f
      _ -> nil
    end
  end

  defp line(c, id_key, item_key) do
    item_id = text(c[id_key])
    %{id: text(c["line_id"]), item_id: item_id, title: get_in(c, [item_key, "title"]) || item_id, quantity: int(c["quantity"], 1)}
  end

  def from_api(a) do
    s = a["market_summary"]

    %{
      id: text(a["catalog_item_id"]),
      title: a["title"] || "",
      subtitle: a["subtitle"],
      description: a["description"] || "",
      badges: for(b <- a["display_badges"] || [], do: text(b["label"])),
      tags: for(t <- a["tags"] || [], do: text(t)),
      categories: for(c <- a["categories"] || [], do: %{name: text(c["name"]), slug: text(c["slug"])}),
      fields: for(f <- a["field_values"] || [], do: %{name: text(f["fieldName"] || f["fieldId"]), value: text(f["value"])}),
      images: for(u <- Enum.take(a["image_urls"] || [], 2), do: text(u)),
      summary:
        s && %{
          lowest: money(s["lowest_price_amount"], s["lowest_price_currency_code"]),
          count: int(s["active_listing_count"], 0),
          quantity: int(s["total_visible_quantity"], 0)
        },
      listings:
        for l <- a["market_listings"] || [] do
          %{
            id: text(l["listing_id"]),
            seller: l["seller_display_name"] || "Seller",
            rating: l["seller_average_rating"],
            reviews: int(l["seller_review_count"], 0),
            condition: l["product_summary"] || "",
            ships_from: l["ship_from_code"] || "",
            quantity: int(l["visible_quantity"], 0),
            price: money(l["price_amount"], l["price_currency_code"]) || "",
            price_value: number(l["price_amount"])
          }
        end,
      offers:
        for o <- a["offer_demand_matches"] || [] do
          %{
            id: text(o["offer_id"]),
            buyer: o["buyer_display_name"] || "Buyer",
            condition: o["product_summary"] || "",
            quantity: int(o["quantity_requested"], 0),
            price: money(o["price_amount"], o["price_currency_code"]) || ""
          }
        end,
      contents: for(c <- a["contents"] || [], do: line(c, "contained_catalog_item_id", "contained_item")),
      included_in: for(c <- a["included_in"] || [], do: line(c, "container_catalog_item_id", "container_item"))
    }
  end

  # Stable, cheapest first; "high to low" is that order reversed, as sortListings does.
  def sort_listings(listings, descending) do
    sorted = Enum.sort_by(listings, & &1.price_value)
    if descending, do: Enum.reverse(sorted), else: sorted
  end

  def alert_message(title, email, target) do
    price =
      case Float.parse(String.trim(target)) do
        {p, ""} -> p
        _ -> nil
      end

    cond do
      not String.contains?(email, "@") -> "Enter a valid email address."
      price == nil or price <= 0 -> "Enter a target price above zero."
      true -> "Alert set: we will email #{email} when #{title} drops below $#{:erlang.float_to_binary(price, decimals: 2)}."
    end
  end
end
