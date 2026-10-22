"""The live benchmark (bench/live): GET /live/prices is a server-sent event stream and every
POST /live/publish body goes to each open stream. Async views, so they need ASGI (uvicorn), where
one process holds every stream and the publisher reaches them all."""

import asyncio

from django.http import HttpResponse, StreamingHttpResponse
from django.views.decorators.csrf import csrf_exempt

_clients: set[asyncio.Queue] = set()


async def prices(request):
    queue: asyncio.Queue = asyncio.Queue()
    _clients.add(queue)

    async def events():
        try:
            yield ": open\n\n"
            while True:
                yield await queue.get()
        finally:
            # Django cancels the stream when the client disconnects.
            _clients.discard(queue)

    return StreamingHttpResponse(events(), content_type="text/event-stream", headers={"Cache-Control": "no-cache"})


@csrf_exempt
async def publish(request):
    event = f"data: {request.body.decode()}\n\n"
    for queue in _clients:
        queue.put_nowait(event)
    return HttpResponse(status=204)
