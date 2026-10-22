class ItemsController < ApplicationController
  def home; end

  def show
    @item = ItemSource.get(params[:id]) or return not_found
    @all_offers = params[:offers] == "all"
    @message = ""
  end

  # Turbo submits the form and replaces only the message; without JavaScript the page comes back.
  def alert
    @item = ItemSource.get(params[:id]) or return not_found
    @message = ItemView.alert_message(params[:title].to_s, params[:email].to_s, params[:target_price].to_s)
    respond_to do |format|
      format.turbo_stream { render turbo_stream: turbo_stream.replace("alert-message", partial: "items/message", locals: { message: @message }) }
      format.html { @all_offers = false; render :show }
    end
  end

  private

  def not_found
    render :not_found, status: :not_found
  end
end
