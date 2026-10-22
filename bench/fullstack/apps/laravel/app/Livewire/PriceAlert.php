<?php

namespace App\Livewire;

use App\Support\ItemView;
use Livewire\Component;

class PriceAlert extends Component
{
    public string $itemId = '';

    public string $title = '';

    public string $email = '';

    public string $target_price = '';

    public string $message = '';

    public function save(): void
    {
        $this->message = ItemView::alertMessage($this->title, $this->email, $this->target_price);
    }

    public function render()
    {
        return view('livewire.price-alert');
    }
}
