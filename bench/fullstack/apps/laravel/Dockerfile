# Laravel Octane on FrankenPHP: the app boots once per worker and stays in memory.
FROM dunglas/frankenphp:1-php8.4-bookworm
# uv: Reverb's event loop on libuv, which is not limited to 1,024 sockets as stream_select is.
RUN install-php-extensions pcntl intl zip opcache uv >/dev/null \
  && cp "$PHP_INI_DIR/php.ini-production" "$PHP_INI_DIR/php.ini"
COPY --from=composer:2 /usr/bin/composer /usr/bin/composer
WORKDIR /app
COPY composer.json composer.lock ./
RUN composer install --no-dev --no-interaction --no-scripts --no-autoloader --prefer-dist -q
COPY . .
RUN cp .env.example .env && composer dump-autoload --optimize --no-dev -q \
  && php artisan key:generate --force -q && php artisan package:discover -q \
  && php artisan octane:install --server=frankenphp -n -q \
  && php artisan route:cache -q && php artisan view:cache -q
EXPOSE 8080
# Two workers for the one CPU the app gets, set explicitly: "auto" is worked out from the CPUs
# the process sees, which a CPU quota (docker --cpus) does not change.
CMD ["php", "artisan", "octane:frankenphp", "--host=0.0.0.0", "--port=8080", "--workers=2"]
