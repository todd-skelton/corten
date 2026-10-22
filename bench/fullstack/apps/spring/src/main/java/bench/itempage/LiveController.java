package bench.itempage;

import java.io.IOException;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;

/**
 * The live benchmark (bench/live): {@code GET /live/prices} is a server-sent event stream and every
 * {@code POST /live/publish} body is sent to each open stream.
 */
@RestController
public class LiveController {
    private final Set<SseEmitter> clients = ConcurrentHashMap.newKeySet();

    @GetMapping(path = "/live/prices", produces = "text/event-stream")
    public SseEmitter prices() throws IOException {
        // 0: no async timeout; the stream stays open until the client goes.
        SseEmitter emitter = new SseEmitter(0L);
        clients.add(emitter);
        emitter.onCompletion(() -> clients.remove(emitter));
        emitter.onError(e -> clients.remove(emitter));
        emitter.send(SseEmitter.event().comment("open"));
        return emitter;
    }

    @PostMapping("/live/publish")
    @ResponseStatus(HttpStatus.NO_CONTENT)
    public void publish(@RequestBody String body) {
        for (SseEmitter emitter : clients) {
            try {
                emitter.send(SseEmitter.event().data(body));
            } catch (IOException | IllegalStateException e) {
                clients.remove(emitter);
            }
        }
    }
}
