Rails.application.routes.draw do
  root "items#home"
  get "items/:id", to: "items#show", as: :item
  post "items/:id", to: "items#alert"
  # The item as JSON for other services: the same data the page renders.
  get "api/items/:id", to: "api/items#show"
  # The live benchmark (bench/live): Action Cable serves /cable; this broadcasts to it.
  post "live/publish", to: "live#publish"
end
