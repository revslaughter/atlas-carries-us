"""The Django URL patterns of the API, which the site includes."""

from django.urls import path

from .views import loan_list

urlpatterns = [path("loans/", loan_list)]
