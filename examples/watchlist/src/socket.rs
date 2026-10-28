//! The WebSocket.
/// A WebSocket that answers each text message in capitals. The handshake is Corten's, checked
/// as a write is (another site's page cannot open it); the frames are tokio-tungstenite's.
pub async fn echo(upgrade: corten::upgrade::Upgrade) -> corten::Result<corten::upgrade::Switch> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::{Message, protocol::Role},
    };
    upgrade.websocket(|stream, signal| async move {
        let mut socket = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
        loop {
            let message = tokio::select! {
                message = socket.next() => message,
                // The app is shutting down: say goodbye rather than drop the connection.
                _ = signal.aborted() => {
                    let _ = socket.close(None).await;
                    break;
                }
            };
            let Some(Ok(Message::Text(text))) = message else { break };
            if socket.send(Message::text(text.as_str().to_uppercase())).await.is_err() {
                break;
            }
        }
    })
}
