FROM python:3.14-slim
WORKDIR /app
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 DJANGO_SETTINGS_MODULE=itempage.settings
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
ADD --checksum=sha256:d6fdc75f204e6bdefa99b69bf1e6d4ac69b8a364f77929f45c13476b4000f717 https://unpkg.com/htmx.org@2.0.11/dist/htmx.min.js static/htmx.min.js
RUN python -m django collectstatic --noinput -v 0
EXPOSE 8080
# Gunicorn's documented worker count, (2 x CPUs) + 1, for one CPU. --preload loads the items once
# before the workers fork.
CMD ["gunicorn", "itempage.wsgi", "--bind", "0.0.0.0:8080", "--workers", "3", "--preload"]
