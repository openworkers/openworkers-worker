//! The `http` crate bridge: a [`Body`] carrying either buffered bytes or,
//! on the 0.3 world, a live stream, and the collector the `TryFrom`
//! conversions use.

use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use bytes::Bytes;
use bytes::BytesMut;
use http_body::Frame;

use crate::Error;
use crate::Result;

/// A body holding its bytes, or pulling them from a stream on the 0.3 world.
#[derive(Debug, Default)]
pub struct Body(BodyInner);

#[derive(Debug, Default)]
enum BodyInner {
    #[default]
    Empty,
    Bytes(Bytes),
    #[cfg(feature = "p3")]
    Stream(crate::streams::ByteStream),
}

impl Body {
    pub fn empty() -> Self {
        Body(BodyInner::Empty)
    }

    pub fn new(bytes: impl Into<Bytes>) -> Self {
        Body(BodyInner::Bytes(bytes.into()))
    }

    #[cfg(feature = "p3")]
    pub(crate) fn from_stream(stream: crate::streams::ByteStream) -> Self {
        Body(BodyInner::Stream(stream))
    }
}

impl Clone for Body {
    fn clone(&self) -> Self {
        match &self.0 {
            BodyInner::Empty => Body(BodyInner::Empty),
            BodyInner::Bytes(bytes) => Body(BodyInner::Bytes(bytes.clone())),
            #[cfg(feature = "p3")]
            BodyInner::Stream(_) => panic!("a streaming body cannot be cloned; read it first"),
        }
    }
}

impl http_body::Body for Body {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        let _ = cx;

        match &mut self.0 {
            BodyInner::Empty => Poll::Ready(None),
            BodyInner::Bytes(bytes) => {
                let bytes = std::mem::take(bytes);
                self.0 = BodyInner::Empty;

                Poll::Ready(Some(Ok(Frame::data(bytes))))
            }
            #[cfg(feature = "p3")]
            BodyInner::Stream(stream) => {
                match futures_util::Stream::poll_next(Pin::new(stream), cx) {
                    Poll::Ready(Some(Ok(chunk))) => {
                        Poll::Ready(Some(Ok(Frame::data(Bytes::from(chunk)))))
                    }
                    Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        matches!(self.0, BodyInner::Empty)
    }
}

impl From<Vec<u8>> for Body {
    fn from(bytes: Vec<u8>) -> Self {
        Body::new(bytes)
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Body::new(bytes)
    }
}

impl From<String> for Body {
    fn from(text: String) -> Self {
        Body::new(text)
    }
}

impl From<&'static str> for Body {
    fn from(text: &'static str) -> Self {
        Body::new(text)
    }
}

/// Reads an `http_body::Body` to its end.
pub(crate) fn collect<B: http_body::Body<Data = Bytes>>(body: B) -> Result<Bytes> {
    crate::rt::poll_to_end(collect_async(body))
        .ok_or_else(|| Error::RustError("body parked with nothing to wake it".into()))?
}

async fn collect_async<B: http_body::Body<Data = Bytes>>(body: B) -> Result<Bytes> {
    let mut body = std::pin::pin!(body);
    let mut collected = BytesMut::new();

    while let Some(frame) = std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
        let frame = frame.map_err(|_| Error::RustError("body could not be read".into()))?;

        if let Ok(data) = frame.into_data() {
            collected.extend_from_slice(&data);
        }
    }

    Ok(collected.freeze())
}
