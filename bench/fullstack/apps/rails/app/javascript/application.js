// Configure your import map in config/importmap.rb. Read more: https://github.com/rails/importmap-rails
import "@hotwired/turbo-rails"
import "controllers"

// The browser benchmark reads this mark: Turbo and the Stimulus controllers are running.
window.__hydratedAt ??= performance.now()
