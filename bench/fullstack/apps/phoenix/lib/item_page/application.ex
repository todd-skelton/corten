defmodule ItemPage.Application do
  # See https://elixir.hexdocs.pm/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    # The fixture (ITEM_SOURCE=fixture) is loaded before the endpoint takes requests.
    :ok = ItemPage.Items.load_fixture()

    children = [
      ItemPageWeb.Telemetry,
      {DNSCluster, query: Application.get_env(:item_page, :dns_cluster_query) || :ignore},
      {Phoenix.PubSub, name: ItemPage.PubSub},
      # Start a worker by calling: ItemPage.Worker.start_link(arg)
      # {ItemPage.Worker, arg},
      # Start to serve requests, typically the last entry
      ItemPageWeb.Endpoint
    ]

    # See https://elixir.hexdocs.pm/Supervisor.html
    # for other strategies and supported options
    opts = [strategy: :one_for_one, name: ItemPage.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Tell Phoenix to update the endpoint configuration
  # whenever the application is updated.
  @impl true
  def config_change(changed, _new, removed) do
    ItemPageWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
