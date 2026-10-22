defmodule ItemPageWeb.PricesChannel do
  @moduledoc "Listing prices for the live benchmark: anyone may join; broadcasts reach every member."
  use Phoenix.Channel

  @impl true
  def join("prices", _payload, socket), do: {:ok, socket}
end
