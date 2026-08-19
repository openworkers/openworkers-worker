//! The `http` crate bridge: a buffered [`Body`] and the collector the
//! `TryFrom` conversions use.

use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use bytes::Bytes;
use bytes::BytesMut;
use http_body::Frame;

use crate::Error;
use crate::Result;

/// A body that already holds all of its bytes.
#[derive(Debug, Clone, Default)]
pub struct Body(Option<Bytes>);

impl Body {
    pub fn empty() -> Self {
        Body(None)
    }

    pub fn new(bytes: impl Into<Bytes>) -> Self {
        Body(Some(bytes.into()))
    }
}

impl http_body::Body for Body {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(self.0.take().map(|bytes| Ok(Frame::data(bytes))))
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_none()
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
