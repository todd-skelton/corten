defmodule ItemPageWeb.HomeLive do
  use ItemPageWeb, :live_view

  def mount(_params, _session, socket), do: {:ok, assign(socket, page_title: "Card Market")}

  def render(assigns) do
    ~H"""
    <h1>Popular items</h1>
    <ul class="home-list">
      <li :for={n <- 1..50}><.link navigate={~p"/items/item_#{n}"}>Card Name {n}</.link></li>
    </ul>
    """
  end
end
