defmodule ItemPageWeb.Router do
  use ItemPageWeb, :router

  pipeline :browser do
    plug :accepts, ["html"]
    plug :fetch_session
    plug :fetch_live_flash
    plug :put_root_layout, html: {ItemPageWeb.Layouts, :root}
    plug :protect_from_forgery
    plug :put_secure_browser_headers
  end

  pipeline :api do
    plug :accepts, ["json"]
  end

  scope "/", ItemPageWeb do
    pipe_through :browser

    live_session :default do
      live "/", HomeLive
      live "/items/:id", ItemLive
    end
  end

  # The item as JSON for other services: the same data the page renders.
  scope "/api", ItemPageWeb do
    pipe_through :api
    get "/items/:id", ItemJSONController, :show
  end

  # The live benchmark's publisher: the JSON body goes to every "prices" subscriber.
  scope "/live", ItemPageWeb do
    pipe_through :api
    post "/publish", LivePublishController, :publish
  end
end
