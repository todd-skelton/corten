<x-layout :title="$item['title'].' | Card Market'">
<article class="item">
<nav class="crumbs">@foreach ($item['categories'] as $c)<a href="/categories/{{ $c['slug'] }}">{{ $c['name'] }}</a>@endforeach</nav>
<h1>{{ $item['title'] }}</h1>
@if ($item['subtitle'])<p class="subtitle">{{ $item['subtitle'] }}</p>@endif
<ul class="badges">@foreach ($item['badges'] as $b)<li>{{ $b }}</li>@endforeach</ul>
<div class="gallery">@foreach ($item['images'] as $i => $url)<div class="img" role="img" aria-label="{{ $item['title'] }} image {{ $i + 1 }}" data-src="{{ $url }}"></div>@endforeach</div>
<section class="summary">@if ($s = $item['summary'])<span><strong>{{ $s['lowest'] ?? '—' }}</strong> lowest</span> <span>{{ number_format($s['count']) }} {{ $s['count'] === 1 ? 'listing' : 'listings' }}</span> <span>{{ number_format($s['quantity']) }} available</span>@else<span>No listings yet</span>@endif</section>
{{-- The server renders the listings cheapest first; "high to low" is the same rows reversed, which is the order sortListings gives. --}}
<section class="listings" x-data="{ descending: false, flip() { const rows = this.$refs.rows; rows.append(...[...rows.rows].reverse()); this.descending = !this.descending } }">
<h2>Listings</h2>
<button type="button" class="sort" @click="flip()" x-text="descending ? 'Price: high to low' : 'Price: low to high'">Price: low to high</button>
<table><thead><tr><th>Seller</th><th>Condition</th><th>Ships from</th><th>Qty</th><th>Price</th></tr></thead>
<tbody x-ref="rows">@foreach ($listings as $l)<tr><td>{{ $l['seller'] }}@if ($l['rating']) · {{ $l['rating'] }} ★ ({{ $l['reviews'] }})@endif</td><td>{{ $l['condition'] }}</td><td>{{ $l['ships_from'] }}</td><td>{{ $l['quantity'] }}</td><td class="price">{{ $l['price'] }}</td></tr>@endforeach</tbody></table>
</section>
<livewire:offers :item-id="$item['id']" />
<livewire:price-alert :item-id="$item['id']" :title="$item['title']" />
<section class="details">
<h2>Details</h2>
<p>{{ $item['description'] }}</p>
<dl>@foreach ($item['fields'] as $f)<dt>{{ $f['name'] }}</dt><dd>{{ $f['value'] }}</dd>@endforeach</dl>
<ul class="tags">@foreach ($item['tags'] as $t)<li>{{ $t }}</li>@endforeach</ul>
</section>
@if (count($item['contents']))<section class="contents"><h2>Contents</h2><ul>@foreach ($item['contents'] as $c)<li><a href="/items/{{ $c['item_id'] }}" wire:navigate>{{ $c['title'] }}</a> × {{ $c['quantity'] }}</li>@endforeach</ul></section>@endif
@if (count($item['included_in']))<section class="included"><h2>Included in</h2><ul>@foreach ($item['included_in'] as $c)<li><a href="/items/{{ $c['item_id'] }}" wire:navigate>{{ $c['title'] }}</a> × {{ $c['quantity'] }}</li>@endforeach</ul></section>@endif
</article>
</x-layout>
