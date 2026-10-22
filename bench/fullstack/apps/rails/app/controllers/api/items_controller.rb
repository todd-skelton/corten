module Api
  class ItemsController < ActionController::API
    def show
      item = ItemSource.get(params[:id])
      item ? render(json: item) : head(:not_found)
    end
  end
end
