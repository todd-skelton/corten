<?php

namespace App\Providers;

use App\Support\ItemSource;
use Illuminate\Support\ServiceProvider;

class AppServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->app->singleton(ItemSource::class);
    }

    public function boot(): void
    {
        // Load the fixture when a server worker boots, not on its first request (and not in the
        // artisan process that starts the workers). PHP's default memory_limit (128M) is too
        // small for the 508 items as PHP arrays, so the in-memory runs fail; that is reported.
        if (! $this->app->runningInConsole()) {
            $this->app->make(ItemSource::class);
        }
    }
}
