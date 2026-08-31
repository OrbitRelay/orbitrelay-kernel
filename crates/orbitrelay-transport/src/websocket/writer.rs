//! WebSocket frame writer. It is the sole owner of the socket sink.

use std::{fmt, time::Duration};

use futures_util::{Sink, SinkExt};
use tokio::{sync::mpsc, time};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use crate::MessageCodec;

use super::{
    coordinator::{CoordinatorEvent, WriterCommand},
    WebSocketAdapterError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriteStatus {
    Completed,
    Cancelled,
}

/// Writes bounded commands to the WebSocket sink.
pub async fn run_writer<S>(
    mut sink: S,
    mut receiver: mpsc::Receiver<WriterCommand>,
    coordinator: mpsc::Sender<CoordinatorEvent>,
    cancellation: CancellationToken,
    write_timeout: Duration,
) -> Result<(), WebSocketAdapterError>
where
    S: Sink<Message> + Send + Unpin,
    S::Error: fmt::Display,
{
    let result = writer_loop(&mut sink, &mut receiver, &cancellation, write_timeout).await;

    if let Err(error) = &result {
        let _ = coordinator
            .send(CoordinatorEvent::WriterFailed {
                error: error.clone(),
            })
            .await;
        cancellation.cancel();
    }

    result
}

async fn writer_loop<S>(
    sink: &mut S,
    receiver: &mut mpsc::Receiver<WriterCommand>,
    cancellation: &CancellationToken,
    write_timeout: Duration,
) -> Result<(), WebSocketAdapterError>
where
    S: Sink<Message> + Unpin,
    S::Error: fmt::Display,
{
    loop {
        let command = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Ok(()),
            command = receiver.recv() => command,
        };

        let Some(command) = command else {
            return close_writer(sink, cancellation, write_timeout).await;
        };

        let status = match command {
            WriterCommand::Application { message, codec } => {
                write_application(sink, &*codec, &message, cancellation, write_timeout).await?
            }
            WriterCommand::NativePing(payload) => {
                send_with_timeout(
                    sink,
                    Message::Ping(payload.into()),
                    "native ping write",
                    cancellation,
                    write_timeout,
                )
                .await?
            }
            WriterCommand::NativePong(payload) => {
                send_with_timeout(
                    sink,
                    Message::Pong(payload.into()),
                    "native pong write",
                    cancellation,
                    write_timeout,
                )
                .await?
            }
            WriterCommand::Close => {
                return close_writer(sink, cancellation, write_timeout).await;
            }
        };

        if status == WriteStatus::Cancelled {
            return Ok(());
        }
    }
}

async fn write_application<S>(
    sink: &mut S,
    codec: &dyn MessageCodec,
    message: &crate::OutboundMessage,
    cancellation: &CancellationToken,
    write_timeout: Duration,
) -> Result<WriteStatus, WebSocketAdapterError>
where
    S: Sink<Message> + Unpin,
    S::Error: fmt::Display,
{
    let encoded = codec.encode_outbound(message)?;
    let text = String::from_utf8(encoded).map_err(|_| {
        WebSocketAdapterError::Task("outbound codec returned non-UTF-8 JSON bytes".to_owned())
    })?;
    send_with_timeout(
        sink,
        Message::Text(text.into()),
        "text write",
        cancellation,
        write_timeout,
    )
    .await
}

async fn close_writer<S>(
    sink: &mut S,
    cancellation: &CancellationToken,
    write_timeout: Duration,
) -> Result<(), WebSocketAdapterError>
where
    S: Sink<Message> + Unpin,
    S::Error: fmt::Display,
{
    if send_with_timeout(
        sink,
        Message::Close(None),
        "close frame write",
        cancellation,
        write_timeout,
    )
    .await?
        == WriteStatus::Cancelled
    {
        return Ok(());
    }

    let _ = close_with_timeout(sink, cancellation, write_timeout).await?;
    Ok(())
}

async fn send_with_timeout<S>(
    sink: &mut S,
    message: Message,
    operation: &'static str,
    cancellation: &CancellationToken,
    write_timeout: Duration,
) -> Result<WriteStatus, WebSocketAdapterError>
where
    S: Sink<Message> + Unpin,
    S::Error: fmt::Display,
{
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Ok(WriteStatus::Cancelled),
        result = time::timeout(write_timeout, sink.send(message)) => {
            map_write_result(result, operation, write_timeout)
        }
    }
}

async fn close_with_timeout<S>(
    sink: &mut S,
    cancellation: &CancellationToken,
    write_timeout: Duration,
) -> Result<WriteStatus, WebSocketAdapterError>
where
    S: Sink<Message> + Unpin,
    S::Error: fmt::Display,
{
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Ok(WriteStatus::Cancelled),
        result = time::timeout(write_timeout, sink.close()) => {
            map_write_result(result, "sink close", write_timeout)
        }
    }
}

fn map_write_result<T, E>(
    result: Result<Result<T, E>, time::error::Elapsed>,
    operation: &'static str,
    write_timeout: Duration,
) -> Result<WriteStatus, WebSocketAdapterError>
where
    E: fmt::Display,
{
    match result {
        Ok(Ok(_)) => Ok(WriteStatus::Completed),
        Ok(Err(error)) => Err(WebSocketAdapterError::Task(format!(
            "WebSocket {operation} failed: {error}"
        ))),
        Err(_) => Err(WebSocketAdapterError::WriteTimeout {
            operation,
            timeout_milliseconds: write_timeout.as_millis().try_into().unwrap_or(u64::MAX),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        pin::Pin,
        sync::{Arc, Mutex},
        task::{Context, Poll},
        time::Duration,
    };

    use futures_util::Sink;
    use tokio::sync::{mpsc, Notify};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_util::sync::CancellationToken;

    use super::run_writer;
    use crate::websocket::{
        coordinator::{CoordinatorEvent, WriterCommand},
        WebSocketAdapterError,
    };

    #[derive(Clone, Copy)]
    enum SinkBehavior {
        Ready,
        PendingFlush,
        FailReady,
    }

    #[derive(Default)]
    struct SinkState {
        messages: Vec<Message>,
        closed: bool,
    }

    struct TestSink {
        behavior: SinkBehavior,
        state: Arc<Mutex<SinkState>>,
        pending: Arc<Notify>,
    }

    impl TestSink {
        fn new(behavior: SinkBehavior) -> (Self, Arc<Mutex<SinkState>>, Arc<Notify>) {
            let state = Arc::new(Mutex::new(SinkState::default()));
            let pending = Arc::new(Notify::new());
            (
                Self {
                    behavior,
                    state: Arc::clone(&state),
                    pending: Arc::clone(&pending),
                },
                state,
                pending,
            )
        }
    }

    #[derive(Debug)]
    struct TestSinkError;

    impl std::fmt::Display for TestSinkError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("test sink failure")
        }
    }

    impl Error for TestSinkError {}

    impl Sink<Message> for TestSink {
        type Error = TestSinkError;

        fn poll_ready(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            match self.behavior {
                SinkBehavior::FailReady => Poll::Ready(Err(TestSinkError)),
                SinkBehavior::Ready | SinkBehavior::PendingFlush => Poll::Ready(Ok(())),
            }
        }

        fn start_send(self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .messages
                .push(item);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            match self.behavior {
                SinkBehavior::PendingFlush => {
                    self.pending.notify_one();
                    Poll::Pending
                }
                SinkBehavior::Ready | SinkBehavior::FailReady => Poll::Ready(Ok(())),
            }
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .closed = true;
            Poll::Ready(Ok(()))
        }
    }

    fn channels() -> (
        mpsc::Sender<WriterCommand>,
        mpsc::Receiver<WriterCommand>,
        mpsc::Sender<CoordinatorEvent>,
        mpsc::Receiver<CoordinatorEvent>,
    ) {
        let (writer_sender, writer_receiver) = mpsc::channel(4);
        let (coordinator_sender, coordinator_receiver) = mpsc::channel(4);
        (
            writer_sender,
            writer_receiver,
            coordinator_sender,
            coordinator_receiver,
        )
    }

    #[tokio::test]
    async fn sends_frames_and_closes_normally() {
        let (sink, state, _) = TestSink::new(SinkBehavior::Ready);
        let (sender, receiver, coordinator, _events) = channels();
        sender
            .send(WriterCommand::NativePing(vec![1, 2, 3]))
            .await
            .expect("writer queue should accept ping");
        sender
            .send(WriterCommand::Close)
            .await
            .expect("writer queue should accept close");

        run_writer(
            sink,
            receiver,
            coordinator,
            CancellationToken::new(),
            Duration::from_secs(1),
        )
        .await
        .expect("writer should finish normally");

        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(matches!(state.messages.first(), Some(Message::Ping(_))));
        assert!(matches!(state.messages.get(1), Some(Message::Close(_))));
        assert!(state.closed);
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_in_flight_send() {
        let (sink, _state, pending) = TestSink::new(SinkBehavior::PendingFlush);
        let (sender, receiver, coordinator, _events) = channels();
        sender
            .send(WriterCommand::NativePing(Vec::new()))
            .await
            .expect("writer queue should accept ping");
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(run_writer(
            sink,
            receiver,
            coordinator,
            task_cancellation,
            Duration::from_secs(5),
        ));
        tokio::time::timeout(Duration::from_secs(1), pending.notified())
            .await
            .expect("send should enter pending flush");

        cancellation.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("cancellation should stop the pending send")
            .expect("writer task should join")
            .expect("cancellation is a normal writer exit");
    }

    #[tokio::test]
    async fn pending_write_times_out_and_cancels_the_connection() {
        let (sink, _state, _) = TestSink::new(SinkBehavior::PendingFlush);
        let (sender, receiver, coordinator, mut events) = channels();
        sender
            .send(WriterCommand::NativePing(Vec::new()))
            .await
            .expect("writer queue should accept ping");
        let cancellation = CancellationToken::new();

        let error = run_writer(
            sink,
            receiver,
            coordinator,
            cancellation.clone(),
            Duration::from_millis(10),
        )
        .await
        .expect_err("pending send should time out");

        assert!(matches!(
            error,
            WebSocketAdapterError::WriteTimeout {
                operation: "native ping write",
                timeout_milliseconds: 10
            }
        ));
        assert!(cancellation.is_cancelled());
        assert!(matches!(
            events.recv().await,
            Some(CoordinatorEvent::WriterFailed {
                error: WebSocketAdapterError::WriteTimeout { .. }
            })
        ));
    }

    #[tokio::test]
    async fn closed_outbound_channel_closes_the_sink() {
        let (sink, state, _) = TestSink::new(SinkBehavior::Ready);
        let (sender, receiver, coordinator, _events) = channels();
        drop(sender);

        run_writer(
            sink,
            receiver,
            coordinator,
            CancellationToken::new(),
            Duration::from_secs(1),
        )
        .await
        .expect("channel closure should close the writer");

        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(matches!(state.messages.first(), Some(Message::Close(_))));
        assert!(state.closed);
    }

    #[tokio::test]
    async fn close_frame_uses_the_write_timeout() {
        let (sink, _state, _) = TestSink::new(SinkBehavior::PendingFlush);
        let (sender, receiver, coordinator, _events) = channels();
        sender
            .send(WriterCommand::Close)
            .await
            .expect("writer queue should accept close");

        let error = run_writer(
            sink,
            receiver,
            coordinator,
            CancellationToken::new(),
            Duration::from_millis(10),
        )
        .await
        .expect_err("close frame should time out");

        assert!(matches!(
            error,
            WebSocketAdapterError::WriteTimeout {
                operation: "close frame write",
                timeout_milliseconds: 10
            }
        ));
    }

    #[tokio::test]
    async fn sink_failure_cancels_the_connection_root() {
        let (sink, _state, _) = TestSink::new(SinkBehavior::FailReady);
        let (sender, receiver, coordinator, mut events) = channels();
        sender
            .send(WriterCommand::NativePong(Vec::new()))
            .await
            .expect("writer queue should accept pong");
        let cancellation = CancellationToken::new();

        let error = run_writer(
            sink,
            receiver,
            coordinator,
            cancellation.clone(),
            Duration::from_secs(1),
        )
        .await
        .expect_err("sink failure should stop the writer");

        assert!(matches!(error, WebSocketAdapterError::Task(_)));
        assert!(cancellation.is_cancelled());
        assert!(matches!(
            events.recv().await,
            Some(CoordinatorEvent::WriterFailed {
                error: WebSocketAdapterError::Task(_)
            })
        ));
    }
}
