<section class="offers">
<h2>Offers</h2>
<table><thead><tr><th>Buyer</th><th>Condition</th><th>Qty</th><th>Price</th></tr></thead>
<tbody>@foreach ($offers as $o)<tr wire:key="{{ $o['id'] }}"><td>{{ $o['buyer'] }}</td><td>{{ $o['condition'] }}</td><td>{{ $o['quantity'] }}</td><td class="price">{{ $o['price'] }}</td></tr>@endforeach</tbody></table>
@if ($total > 5 && ! $all)<button type="button" class="more" wire:click="showAll">Show all {{ $total }} offers</button>@endif
</section>
