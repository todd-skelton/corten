using System.Collections.Concurrent;
using System.Net.ServerSentEvents;
using System.Runtime.CompilerServices;
using System.Threading.Channels;

namespace ItemPage;

/// <summary>
/// The live benchmark (bench/live): every open <c>GET /live/prices</c> stream has a channel, and
/// <see cref="Publish"/> writes each <c>POST /live/publish</c> body to all of them.
/// </summary>
public sealed class LivePrices
{
    private readonly ConcurrentDictionary<Channel<string>, byte> _clients = new();

    public async IAsyncEnumerable<SseItem<string>> Subscribe([EnumeratorCancellation] CancellationToken cancel)
    {
        var channel = Channel.CreateUnbounded<string>(new UnboundedChannelOptions { SingleReader = true });
        _clients.TryAdd(channel, 0);
        try
        {
            // An event first, so the headers go out now rather than with the first price.
            yield return new SseItem<string>("", "open");
            await foreach (var message in channel.Reader.ReadAllAsync(cancel))
            {
                yield return new SseItem<string>(message);
            }
        }
        finally
        {
            _clients.TryRemove(channel, out _);
        }
    }

    public void Publish(string message)
    {
        foreach (var channel in _clients.Keys)
        {
            channel.Writer.TryWrite(message);
        }
    }
}
