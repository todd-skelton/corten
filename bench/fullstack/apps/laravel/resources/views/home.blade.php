<x-layout>
<h1>Popular items</h1>
<ul class="home-list">@foreach (range(1, 50) as $n)<li><a href="/items/item_{{ $n }}" wire:navigate>Card Name {{ $n }}</a></li>@endforeach</ul>
</x-layout>
