"""The Django URL patterns of the site."""

from django.urls import include, path

from . import views

urlpatterns = [
    path("books/", views.book_list, name="books"),
    path("books/<int:pk>/", views.BookView.as_view(), name="book"),
    path("api/", include("lending.web.api_urls")),
]
