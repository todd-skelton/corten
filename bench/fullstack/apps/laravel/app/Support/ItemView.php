<?php

namespace App\Support;

/** Same view model and rules as sveltekit/src/lib/view-model.ts. */
class ItemView
{
    private static function text(mixed $v): string
    {
        return $v === null ? '' : (is_string($v) ? $v : json_encode($v));
    }

    private static function money(mixed $amount, mixed $code): ?string
    {
        if ($amount === null || $code === null) {
            return null;
        }

        return $code === 'USD' ? '$'.self::text($amount) : self::text($amount).' '.self::text($code);
    }

    private static function line(array $c, string $idKey, string $itemKey): array
    {
        $itemId = self::text($c[$idKey] ?? null);

        return ['id' => self::text($c['line_id'] ?? null), 'item_id' => $itemId, 'title' => $c[$itemKey]['title'] ?? $itemId, 'quantity' => (int) ($c['quantity'] ?? 1)];
    }

    public static function fromApi(array $a): array
    {
        $s = $a['market_summary'] ?? null;

        return [
            'id' => self::text($a['catalog_item_id']),
            'title' => $a['title'] ?? '',
            'subtitle' => $a['subtitle'] ?? null,
            'description' => $a['description'] ?? '',
            'badges' => array_map(fn ($b) => self::text($b['label']), $a['display_badges'] ?? []),
            'tags' => array_map(self::text(...), $a['tags'] ?? []),
            'categories' => array_map(fn ($c) => ['name' => self::text($c['name']), 'slug' => self::text($c['slug'])], $a['categories'] ?? []),
            'fields' => array_map(fn ($f) => ['name' => self::text($f['fieldName'] ?? $f['fieldId']), 'value' => self::text($f['value'])], $a['field_values'] ?? []),
            'images' => array_map(self::text(...), array_slice($a['image_urls'] ?? [], 0, 2)),
            'summary' => $s === null ? null : [
                'lowest' => self::money($s['lowest_price_amount'] ?? null, $s['lowest_price_currency_code'] ?? null),
                'count' => (int) $s['active_listing_count'],
                'quantity' => (int) $s['total_visible_quantity'],
            ],
            'listings' => array_map(fn ($l) => [
                'id' => self::text($l['listing_id']),
                'seller' => $l['seller_display_name'] ?? 'Seller',
                'rating' => $l['seller_average_rating'] ?? null,
                'reviews' => (int) ($l['seller_review_count'] ?? 0),
                'condition' => $l['product_summary'] ?? '',
                'ships_from' => $l['ship_from_code'] ?? '',
                'quantity' => (int) ($l['visible_quantity'] ?? 0),
                'price' => self::money($l['price_amount'] ?? null, $l['price_currency_code'] ?? null) ?? '',
                'price_value' => (float) $l['price_amount'],
            ], $a['market_listings'] ?? []),
            'offers' => array_map(fn ($o) => [
                'id' => self::text($o['offer_id']),
                'buyer' => $o['buyer_display_name'] ?? 'Buyer',
                'condition' => $o['product_summary'] ?? '',
                'quantity' => (int) ($o['quantity_requested'] ?? 0),
                'price' => self::money($o['price_amount'] ?? null, $o['price_currency_code'] ?? null) ?? '',
            ], $a['offer_demand_matches'] ?? []),
            'contents' => array_map(fn ($c) => self::line($c, 'contained_catalog_item_id', 'contained_item'), $a['contents'] ?? []),
            'included_in' => array_map(fn ($c) => self::line($c, 'container_catalog_item_id', 'container_item'), $a['included_in'] ?? []),
        ];
    }

    /** Cheapest first, stable (PHP's sort is stable since 8.0). */
    public static function sortedListings(array $item): array
    {
        $listings = $item['listings'];
        usort($listings, fn ($a, $b) => $a['price_value'] <=> $b['price_value']);

        return $listings;
    }

    public static function alertMessage(string $title, string $email, string $target): string
    {
        $t = trim($target);
        $price = is_numeric($t) ? (float) $t : NAN;
        if (! str_contains($email, '@')) {
            return 'Enter a valid email address.';
        }
        if (! ($price > 0)) {
            return 'Enter a target price above zero.';
        }

        return "Alert set: we will email {$email} when {$title} drops below \$".number_format($price, 2, '.', '').'.';
    }
}
