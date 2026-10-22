<?php

use App\Events\PriceTick;
use App\Support\ItemSource;
use App\Support\ItemView;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;

Route::view('/', 'home');

Route::get('/items/{id}', function (string $id, ItemSource $items) {
    $item = $items->get($id) ?? abort(404);

    return view('item', ['item' => $item, 'listings' => ItemView::sortedListings($item)]);
});

// The live benchmark (bench/live): one price update, broadcast now (no queue) to Reverb.
Route::post('/live/publish', function (Request $request) {
    broadcast(new PriceTick((int) $request->input('seq'), (float) $request->input('sent'), (string) $request->input('price')));

    return response()->noContent();
});
