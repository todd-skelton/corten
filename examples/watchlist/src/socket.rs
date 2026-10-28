//! The `/quotes` WebSocket: a ticker outside the browser (a menu-bar app, a watch face) names the
//! stocks it shows and is sent each one's price as it trades. An `Upgrade` example.
use std::future::Future;

use corten::{
    prelude::*,
    upgrade::{Switch, Upgrade},
};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        protocol::{Role, WebSocketConfig},
    },
};

use crate::market::Market;

/// The longest message a client sends: a ticker. Anything longer closes the socket rather than
/// being read into memory.
const LONGEST: usize = 64;

/// A client sends a ticker (`ACME`) to follow it, and gets the stock's feed row at once (the same
/// JSON as `/feed/stocks`), then again each time its price moves. A ticker the market does not
/// list is answered `{"unknown":"ZZZZ"}`. The handshake is Corten's, checked as a write is, so
/// another site's page cannot open it; the frames are tokio-tungstenite's.
pub(crate) async fn quotes(State(market): State<Market>, upgrade: Upgrade) -> corten::Result<Switch> {
    upgrade.websocket(|stream, signal| async move {
        let config = WebSocketConfig::default().max_message_size(Some(LONGEST)).max_frame_size(Some(LONGEST));
        let socket = WebSocketStream::from_raw_socket(stream, Role::Server, Some(config)).await;
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
                    let asked = text.as_str().trim();
                    match Market::ticker(asked) {
                        Some(ticker) => {
                            let stock = market.stocks.read(|rows| rows[ticker].clone());
                            if !followed.iter().any(|(id, _)| *id == ticker) {
                                followed.push((ticker, stock.price_cents));
                            }
                            socket.send(Message::text(stock.to_json())).await
                        }
                        None => socket.send(Message::text(format!(r#"{{"unknown":{}}}"#, asked.to_json()))).await,
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                // Pings are answered by the library; a binary frame means nothing here.
                Some(Ok(_)) => Ok(()),
            },
            changed = trades.changed() => {
                if changed.is_err() {
                    break;
                }
                let mut sent = Ok(());
                for (ticker, price) in &mut followed {
                    let stock = market.stocks.read(|rows| rows[*ticker].clone());
                    if stock.price_cents != *price {
                        *price = stock.price_cents;
                        sent = socket.send(Message::text(stock.to_json())).await;
                        if sent.is_err() {
                            break;
                        }
                    }
                }
                sent
            }
            () = &mut stop => {
                let _ = socket.close(None).await;
                break;
            }
        };
        if sent.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client on the other end of an in-memory connection.
    async fn connect(market: &Market) -> WebSocketStream<tokio::io::DuplexStream> {
        let (client, server) = tokio::io::duplex(4096);
        let market = market.clone();
        tokio::spawn(async move {
            let socket = WebSocketStream::from_raw_socket(server, Role::Server, None).await;
            follow(socket, market, std::future::pending()).await;
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
        let mut client = connect(&market).await;
        client.send(Message::text("ACME")).await.unwrap();
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
        let mut client = connect(&market).await;
        client.send(Message::text("GLBX")).await.unwrap();
        next(&mut client).await;
        // The first trade is Acme's, the second Halcyon Health's: neither is followed.
        market.trade(0);
        market.trade(1);
        client.send(Message::text("GLBX")).await.unwrap();
        let again = next(&mut client).await;
        assert!(again.contains(r#""id":"GLBX""#), "the next message is the answer, not a trade: {again}");
    }
}
