//! WebSocket frame reader. It only frames and classifies network input.

use futures_util::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use tokio_util::sync::CancellationToken;

use super::coordinator::ReaderEvent;

/// Reads frames into the bounded coordinator channel.
pub async fn run_reader<S>(
    mut stream: futures_util::stream::SplitStream<WebSocketStream<S>>,
    sender: mpsc::Sender<ReaderEvent>,
    cancellation: CancellationToken,
    max_message_bytes: usize,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            next = stream.next() => next,
        };

        let Some(frame) = next else {
            let _ = sender.send(ReaderEvent::Closed).await;
            return Ok(());
        };

        let frame = frame.map_err(|error| format!("WebSocket reader failed: {error}"))?;
        match frame {
            Message::Text(text) => {
                if text.len() > max_message_bytes {
                    let _ = sender.send(ReaderEvent::Oversized).await;
                    return Ok(());
                }
                sender
                    .send(ReaderEvent::Text(text.as_bytes().to_vec()))
                    .await
                    .map_err(|_| "coordinator input channel closed".to_owned())?;
            }
            Message::Binary(bytes) => {
                if bytes.len() > max_message_bytes {
                    let _ = sender.send(ReaderEvent::Oversized).await;
                } else {
                    let _ = sender.send(ReaderEvent::Binary).await;
                }
                return Ok(());
            }
            Message::Ping(payload) => {
                sender
                    .send(ReaderEvent::NativePing(payload.to_vec()))
                    .await
                    .map_err(|_| "coordinator input channel closed".to_owned())?;
            }
            Message::Pong(_) => {
                sender
                    .send(ReaderEvent::NativePong)
                    .await
                    .map_err(|_| "coordinator input channel closed".to_owned())?;
            }
            Message::Close(_) => {
                let _ = sender.send(ReaderEvent::Closed).await;
                return Ok(());
            }
            Message::Frame(_) => {}
        }
    }
}
