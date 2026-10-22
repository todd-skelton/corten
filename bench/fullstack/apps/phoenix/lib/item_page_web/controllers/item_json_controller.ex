defmodule ItemPageWeb.ItemJSONController do
  use ItemPageWeb, :controller

  def show(conn, %{"id" => id}) do
    case ItemPage.Items.get(id) do
      nil -> conn |> put_status(:not_found) |> json(%{error: "Not found"})
      item -> json(conn, item)
    end
  end
end
