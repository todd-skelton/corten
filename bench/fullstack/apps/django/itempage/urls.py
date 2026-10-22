from django.urls import path

from . import live, views

urlpatterns = [
    path("", views.home),
    path("items/<str:item_id>", views.item),
    path("api/items/<str:item_id>", views.item_json),
    path("live/prices", live.prices),
    path("live/publish", live.publish),
]
