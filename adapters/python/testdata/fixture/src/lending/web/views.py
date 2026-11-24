"""Django views of the library."""

from django.http import JsonResponse
from django.views import View


def book_list(request):
    """Lists the books."""
    return JsonResponse({"books": []})


def loan_list(request):
    """Lists the loans."""
    return JsonResponse({"loans": []})


class BookView(View):
    """Shows or changes one book."""

    def get(self, request, pk: int):
        """Shows one book."""
        return JsonResponse({"id": pk})

    def post(self, request, pk: int):
        """Changes one book."""
        return JsonResponse({"id": pk})
