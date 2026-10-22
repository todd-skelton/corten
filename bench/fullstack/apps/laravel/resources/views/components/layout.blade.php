@props(['title' => 'Card Market'])
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ $title }}</title>
<link rel="stylesheet" href="{{ asset('style.css') }}">
@livewireStyles
<script>
// The browser benchmark reads this mark: Livewire and Alpine have started.
document.addEventListener('livewire:initialized', () => { window.__hydratedAt ??= performance.now(); });
</script>
</head>
<body>
<header class="site"><a href="/" wire:navigate>Card Market</a></header>
<main>{{ $slot }}</main>
@livewireScripts
</body>
</html>
