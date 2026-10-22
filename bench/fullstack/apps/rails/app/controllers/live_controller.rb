# The live benchmark's publisher: the JSON body goes to every PricesChannel subscriber.
class LiveController < ActionController::Base
  # Another service publishes here, not a browser form.
  skip_forgery_protection

  def publish
    ActionCable.server.broadcast("prices", JSON.parse(request.raw_post))
    head :no_content
  end
end
