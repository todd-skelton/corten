<?php

return [
    // fixture: API responses from FIXTURE_PATH, converted once per worker at boot, so the benchmark
    // measures the web server alone. Otherwise every request fetches the item from the item API.
    'source' => env('ITEM_SOURCE', 'api'),
    'fixture_path' => env('FIXTURE_PATH', 'fixtures/items.json'),
    'api_base' => env('API_BASE', 'http://localhost:8080'),
];
