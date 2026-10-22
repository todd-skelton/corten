<?php

namespace App\Support;

use Illuminate\Support\Facades\Http;

class ItemSource
{
    /** @var array<string, array>|null */
    private ?array $fixture = null;

    public function __construct()
    {
        if (config('items.source') === 'fixture') {
            $raw = json_decode(file_get_contents(config('items.fixture_path')), true);
            $this->fixture = array_map(ItemView::fromApi(...), $raw);
        }
    }

    public function get(string $id): ?array
    {
        if ($this->fixture !== null) {
            return $this->fixture[$id] ?? null;
        }
        $res = Http::get(config('items.api_base').'/api/discovery/items/'.rawurlencode($id));
        if ($res->status() === 404) {
            return null;
        }
        $res->throw();

        return ItemView::fromApi($res->json());
    }
}
