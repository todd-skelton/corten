//! The `/quotes` WebSocket: a ticker outside the browser (a menu-bar app, a watch face) names the
//! stocks it shows and is sent each one's price as it trades. An `Upgrade` example.
use std::{future::Future, time::Duration};

use corten::{
    prelude::*,
    upgrade::{Switch, Upgrade},
};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Error, Message,
        protocol::{CloseFrame, Role, WebSocketConfig, frame::coding::CloseCode},
    },
};

use crate::market::{Market, Stock};

/// The longest message a client sends: a ticker. Anything longer closes the socket (1009, too
/// big) rather than being read into memory.
const LONGEST: usize = 64;

/// How long a client may take to read what it is sent; a client that stops reading is let go
/// rather than hold its task until the app stops.
const SLOW: Duration = Duration::from_secs(10);

/// Small buffers for small messages. A frame may still be up to 125 bytes, the most a control
/// frame (a ping, a close with its reason) can be.
fn config() -> WebSocketConfig {
    WebSocketConfig::default().max_message_size(Some(LONGEST)).max_frame_size(Some(125)).read_buffer_size(1024)
}

/// A client sends a ticker (`ACME`, in any case) to follow it, and gets the stock's feed row at
/// once (the same JSON as `/feed/stocks`), then again each time its price moves. A ticker the
/// market does not list is answered `{"unknown":"ZZZZ"}`. The handshake is Corten's, checked as
/// a write is, so another site's page cannot open it; the frames are tokio-tungstenite's.
pub(crate) async fn quotes(State(market): State<Market>, upgrade: Upgrade) -> corten::Result<Switch> {
    upgrade.websocket(|stream, signal| async move {
        let socket = WebSocketStream::from_raw_socket(stream, Role::Server, Some(config())).await;
        // The app is shutting down: say goodbye rather than drop the connection.
        follow(socket, market, async move {
            signal.aborted().await;
        })
        .await;
    })
}

/// Answers the client's tickers and sends each followed stock when it trades, until the client
/// leaves or `stop` resolves.
async fn follow<S>(mut socket: WebSocketStream<S>, market: Market, stop: impl Future<Output = ()>)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut trades = market.trades.subscribe();
    // Each stock followed, with the price last sent for it.
    let mut followed: Vec<(&'static str, u32)> = Vec::new();
    let mut stop = std::pin::pin!(stop);
    loop {
        let sent = tokio::select! {
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let asked = text.as_str().trim().to_ascii_uppercase();
                    match Market::ticker(&asked) {
                        Some(ticker) => {
                            let stock = market.stocks.read(|rows| rows[ticker].clone());
                            match followed.iter_mut().find(|(id, _)| *id == ticker) {
                                Some((_, price)) => *price = stock.price_cents,
                                None => followed.push((ticker, stock.price_cents)),
                            }
                            send(&mut socket, stock.to_json()).await
                        }
                        None => send(&mut socket, format!(r#"{{"unknown":{}}}"#, asked.to_json())).await,
                    }
                }
                // The client says goodbye: the library queued the answer, and a flush sends it
                // (a close of our own would be refused now).
                Some(Ok(Message::Close(_))) => {
                    let _ = tokio::time::timeout(SLOW, socket.flush()).await;
                    break;
                }
                Some(Err(Error::Capacity(_))) => {
                    let too_big = CloseFrame { code: CloseCode::Size, reason: "a ticker is at most 64 bytes".into() };
                    goodbye(&mut socket, Some(too_big)).await;
                    break;
                }
                Some(Err(_)) | None => break,
                // Pings are answered by the library; a binary frame means nothing here.
                Some(Ok(_)) => Ok(()),
            },
            changed = trades.changed() => {
                if changed.is_err() {
                    break;
                }
                // The followed stocks whose price moved since it was sent, read at once.
                let moved: Vec<Stock> = market.stocks.read(|rows| {
                    followed
                        .iter_mut()
                        .filter_map(|(id, price)| {
                            let stock = &rows[*id];
                            (stock.price_cents != *price).then(|| {
                                *price = stock.price_cents;
                                stock.clone()
                            })
                        })
                        .collect()
                });
                let mut sent = Ok(());
                for stock in moved {
                    sent = send(&mut socket, stock.to_json()).await;
                    if sent.is_err() {
                        break;
                    }
                }
                sent
            }
            () = &mut stop => {
                goodbye(&mut socket, None).await;
                break;
            }
        };
        if sent.is_err() {
            break;
        }
    }
}

/// Sends `text`, unless the client does not read it in time.
async fn send<S: AsyncRead + AsyncWrite + Unpin>(socket: &mut WebSocketStream<S>, text: String) -> Result<(), ()> {
    match tokio::time::timeout(SLOW, socket.send(Message::text(text))).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

/// Closes the socket with `frame`.
async fn goodbye<S: AsyncRead + AsyncWrite + Unpin>(socket: &mut WebSocketStream<S>, frame: Option<CloseFrame>) {
    let _ = tokio::time::timeout(SLOW, socket.close(frame)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client on the other end of an in-memory connection; the server stops when `stop` does.
    async fn connect(
        market: &Market,
        stop: impl Future<Output = ()> + Send + 'static,
    ) -> WebSocketStream<tokio::io::DuplexStream> {
        let (client, server) = tokio::io::duplex(4096);
        let market = market.clone();
        tokio::spawn(async move {
            let socket = WebSocketStream::from_raw_socket(server, Role::Server, Some(config())).await;
            follow(socket, market, stop).await;
        });
        WebSocketStream::from_raw_socket(client, Role::Client, None).await
    }

    async fn next(client: &mut WebSocketStream<tokio::io::DuplexStream>) -> String {
        match client.next().await {
            Some(Ok(Message::Text(text))) => text.as_str().to_string(),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_followed_stock_is_sent_at_once_and_again_when_it_trades() {
        let market = Market::new();
        let mut client = connect(&market, std::future::pending()).await;
        client.send(Message::text("acme")).await.unwrap();
        let first = next(&mut client).await;
        assert!(first.contains(r#""id":"ACME""#) && first.contains(r#""price":"$129.99""#), "{first}");
        client.send(Message::text("ZZZZ")).await.unwrap();
        assert_eq!(next(&mut client).await, r#"{"unknown":"ZZZZ"}"#);
        // The first trade moves Acme's price.
        market.trade(0);
        let moved = next(&mut client).await;
        assert!(moved.contains(r#""price":"$130.44""#), "{moved}");
    }

    #[tokio::test]
    async fn a_trade_of_a_stock_not_followed_sends_nothing() {
        let market = Market::new();
        let mut client = connect(&market, std::future::pending()).await;
        client.send(Message::text("GLBX")).await.unwrap();
        next(&mut client).await;
        // Acme's trade, then Halcyon Health's: neither is followed. Then Globex's, down 45 basis
        // points of $540.00, which is the next message the client gets.
        market.trade(0);
        market.trade(1);
        market.trade(3);
        let next = next(&mut client).await;
        assert!(next.contains(r#""id":"GLBX""#) && next.contains(r#""price":"$537.57""#), "{next}");
    }

    #[tokio::test]
    async fn a_message_longer_than_a_ticker_closes_the_socket_as_too_big() {
        let market = Market::new();
        let mut client = connect(&market, std::future::pending()).await;
        client.send(Message::text("A".repeat(LONGEST + 1))).await.unwrap();
        match client.next().await {
            Some(Ok(Message::Close(Some(frame)))) => assert_eq!(frame.code, CloseCode::Size),
            other => panic!("expected a close, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_server_says_goodbye_when_it_stops_and_answers_the_clients() {
        let market = Market::new();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let mut client = connect(&market, async move {
            let _ = stopped.await;
        })
        .await;
        client.send(Message::text("ACME")).await.unwrap();
        next(&mut client).await;
        stop.send(()).unwrap();
        assert!(matches!(client.next().await, Some(Ok(Message::Close(_)))), "shutting down closes the socket");

        let mut client = connect(&market, std::future::pending()).await;
        client.close(None).await.unwrap();
        let answer = client.next().await;
        assert!(matches!(answer, Some(Ok(Message::Close(_)))), "the client's close is answered: {answer:?}");
    }
}
