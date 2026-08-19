//! `ByteStream`, what `Request::stream` and `Response::stream` hand out. On
//! the 0.2 world the host buffers bodies at the boundary, so the stream
//! yields one prebuffered chunk; on the 0.3 world it pulls chunks from a
//! live `stream<u8>` as the host delivers them.

use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use futures_util::Stream;

use crate::Result;

/// Bodies flow through in pieces of this size, so peak memory stays a
/// chunk, not a body
#[cfg(feature = "p3")]
const CHUNK_SIZE: usize = 64 * 1024;

pub struct ByteStream {
    inner: Inner,
}

enum Inner {
    Fixed(Option<Vec<u8>>),
    #[cfg(feature = "p3")]
    Live(IncomingStream),
    #[cfg(feature = "p3")]
    Boxed(Pin<Box<dyn Stream<Item = Result<Vec<u8>>>>>),
}

impl ByteStream {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        ByteStream {
            inner: Inner::Fixed(Some(bytes)),
        }
    }

    #[cfg(feature = "p3")]
    pub(crate) fn live(stream: IncomingStream) -> Self {
        ByteStream {
            inner: Inner::Live(stream),
        }
    }

    #[cfg(feature = "p3")]
    pub(crate) fn boxed<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Vec<u8>>> + 'static,
    {
        ByteStream {
            inner: Inner::Boxed(Box::pin(stream)),
        }
    }

    /// Reads the stream to its end as one buffer.
    #[cfg(feature = "p3")]
    pub(crate) async fn collect_bytes(mut self) -> Result<Vec<u8>> {
        use futures_util::StreamExt;

        let mut bytes = Vec::new();

        while let Some(chunk) = self.next().await {
            bytes.extend_from_slice(&chunk?);
        }

        Ok(bytes)
    }
}

impl std::fmt::Debug for ByteStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Inner::Fixed(chunk) => f.debug_tuple("ByteStream").field(chunk).finish(),
            #[cfg(feature = "p3")]
            Inner::Live(_) => f.write_str("ByteStream(live)"),
            #[cfg(feature = "p3")]
            Inner::Boxed(_) => f.write_str("ByteStream(stream)"),
        }
    }
}

impl Stream for ByteStream {
    type Item = Result<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let _ = cx;

        match &mut self.inner {
            Inner::Fixed(chunk) => Poll::Ready(chunk.take().map(Ok)),
            #[cfg(feature = "p3")]
            Inner::Live(stream) => Pin::new(stream).poll_next(cx),
            #[cfg(feature = "p3")]
            Inner::Boxed(stream) => stream.as_mut().poll_next(cx),
        }
    }
}

/// A guest-side reader over a component-model `stream<u8>`, yielding chunks
/// as `futures_util::Stream` items so poll-based consumers (`ByteStream`,
/// `Body`) can drive it.
#[cfg(feature = "p3")]
pub(crate) struct IncomingStream {
    state: ReadState,
}

#[cfg(feature = "p3")]
type Reader = wit_bindgen::rt::async_support::StreamReader<u8>;

/// One read in flight: the reader rides inside the future and comes back
/// with the chunk
#[cfg(feature = "p3")]
type PendingRead = Pin<Box<dyn std::future::Future<Output = (Option<Vec<u8>>, Option<Reader>)>>>;

#[cfg(feature = "p3")]
enum ReadState {
    Idle(Reader),
    Reading(PendingRead),
    Done,
}

#[cfg(feature = "p3")]
impl IncomingStream {
    pub(crate) fn new(reader: Reader) -> Self {
        IncomingStream {
            state: ReadState::Idle(reader),
        }
    }
}

#[cfg(feature = "p3")]
impl Stream for IncomingStream {
    type Item = Result<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use wit_bindgen::rt::async_support::StreamResult;

        loop {
            match std::mem::replace(&mut self.state, ReadState::Done) {
                ReadState::Idle(mut reader) => {
                    self.state = ReadState::Reading(Box::pin(async move {
                        loop {
                            let (result, buf) = reader.read(Vec::with_capacity(CHUNK_SIZE)).await;

                            match result {
                                StreamResult::Complete(_) if buf.is_empty() => continue,
                                StreamResult::Complete(_) => break (Some(buf), Some(reader)),
                                StreamResult::Dropped if buf.is_empty() => break (None, None),
                                StreamResult::Dropped => break (Some(buf), None),
                                StreamResult::Cancelled => break (None, None),
                            }
                        }
                    }));
                }
                ReadState::Reading(mut future) => match future.as_mut().poll(cx) {
                    Poll::Ready((chunk, reader)) => {
                        self.state = match reader {
                            Some(reader) => ReadState::Idle(reader),
                            None => ReadState::Done,
                        };

                        return Poll::Ready(chunk.map(Ok));
                    }
                    Poll::Pending => {
                        self.state = ReadState::Reading(future);

                        return Poll::Pending;
                    }
                },
                ReadState::Done => return Poll::Ready(None),
            }
        }
    }
}
