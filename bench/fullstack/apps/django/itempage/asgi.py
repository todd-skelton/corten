# ASGI entry point, for the live benchmark's stream (bench/live runs uvicorn with this).
import os

from django.core.asgi import get_asgi_application

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "itempage.settings")
application = get_asgi_application()
