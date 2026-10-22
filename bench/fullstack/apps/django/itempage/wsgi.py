from django.core.wsgi import get_wsgi_application

application = get_wsgi_application()

# Load the items (and the URLs) now, so gunicorn's --preload shares them with every worker.
import itempage.urls  # noqa: E402,F401
