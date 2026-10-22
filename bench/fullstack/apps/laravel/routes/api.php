<?php

use App\Support\ItemSource;
use Illuminate\Support\Facades\Route;

// The item as JSON for other services: the same data the page renders.
Route::get('/items/{id}', fn (string $id, ItemSource $items) => response()->json($items->get($id) ?? abort(404)));
