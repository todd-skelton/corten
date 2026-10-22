defmodule ItemPageWeb.NotFound do
  @moduledoc "Raised for an unknown item; Phoenix answers it with its 404 page."
  defexception message: "Not found", plug_status: 404
end
