# Listing prices for the live benchmark: every subscriber gets each broadcast to "prices".
class PricesChannel < ApplicationCable::Channel
  def subscribed
    stream_from "prices"
  end
end
