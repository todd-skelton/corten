<?php

namespace App\Livewire;

use App\Support\ItemSource;
use Livewire\Component;

class Offers extends Component
{
    public string $itemId = '';

    public bool $all = false;

    public function showAll(): void
    {
        $this->all = true;
    }

    public function render()
    {
        $offers = app(ItemSource::class)->get($this->itemId)['offers'] ?? [];

        return view('livewire.offers', ['offers' => $this->all ? $offers : array_slice($offers, 0, 5), 'total' => count($offers)]);
    }
}
