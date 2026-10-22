defmodule ItemPageWeb.LivePublishController do
  use Phoenix.Controller, formats: [:json]

  def publish(conn, params) do
    ItemPageWeb.Endpoint.broadcast("prices", "tick", params)
    send_resp(conn, 204, "")
  end
end
