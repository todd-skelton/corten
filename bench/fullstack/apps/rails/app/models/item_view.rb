# Same view model and rules as sveltekit/src/lib/view-model.ts, as a Hash so the JSON route
# renders it as is.
module ItemView
  module_function

  def money(amount, code)
    return nil if amount.nil? || code.nil?
    code == "USD" ? "$#{amount}" : "#{amount} #{code}"
  end

  def line(c, id_key, item_key)
    item_id = c[id_key].to_s
    { "id" => c["line_id"].to_s, "item_id" => item_id, "title" => c.dig(item_key, "title") || item_id, "quantity" => (c["quantity"] || 1).to_i }
  end

  def from_api(a)
    s = a["market_summary"]
    {
      "id" => a["catalog_item_id"].to_s,
      "title" => a["title"] || "",
      "subtitle" => a["subtitle"],
      "description" => a["description"] || "",
      "badges" => (a["display_badges"] || []).map { |b| b["label"].to_s },
      "tags" => (a["tags"] || []).map(&:to_s),
      "categories" => (a["categories"] || []).map { |c| { "name" => c["name"].to_s, "slug" => c["slug"].to_s } },
      "fields" => (a["field_values"] || []).map do |f|
        { "name" => (f["fieldName"].nil? ? f["fieldId"] : f["fieldName"]).to_s, "value" => f["value"].is_a?(String) ? f["value"] : JSON.generate(f["value"]) }
      end,
      "images" => (a["image_urls"] || []).first(2).map(&:to_s),
      "summary" => s && {
        "lowest" => money(s["lowest_price_amount"], s["lowest_price_currency_code"]),
        "count" => s["active_listing_count"].to_i,
        "quantity" => s["total_visible_quantity"].to_i
      },
      "listings" => (a["market_listings"] || []).map do |l|
        {
          "id" => l["listing_id"].to_s, "seller" => l["seller_display_name"] || "Seller", "rating" => l["seller_average_rating"],
          "reviews" => (l["seller_review_count"] || 0).to_i, "condition" => l["product_summary"] || "", "ships_from" => l["ship_from_code"] || "",
          "quantity" => (l["visible_quantity"] || 0).to_i, "price" => money(l["price_amount"], l["price_currency_code"]) || "",
          "price_value" => Float(l["price_amount"], exception: false) || Float::NAN
        }
      end,
      "offers" => (a["offer_demand_matches"] || []).map do |o|
        {
          "id" => o["offer_id"].to_s, "buyer" => o["buyer_display_name"] || "Buyer", "condition" => o["product_summary"] || "",
          "quantity" => (o["quantity_requested"] || 0).to_i, "price" => money(o["price_amount"], o["price_currency_code"]) || ""
        }
      end,
      "contents" => (a["contents"] || []).map { |c| line(c, "contained_catalog_item_id", "contained_item") },
      "included_in" => (a["included_in"] || []).map { |c| line(c, "container_catalog_item_id", "container_item") }
    }.freeze
  end

  def alert_message(title, email, target)
    price = Float(target.to_s.strip, exception: false) || Float::NAN
    return "Enter a valid email address." unless email.include?("@")
    return "Enter a target price above zero." unless price > 0
    "Alert set: we will email #{email} when #{title} drops below $#{format("%.2f", price)}."
  end
end
