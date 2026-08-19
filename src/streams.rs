//! `ByteStream`, kept so that `Request::stream` and `Response::stream` have
//! somewhere to hand their bytes. The host buffers bodies at the boundary,
//! so the stream yields what is already in memory in one chunk.

use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use futures_util::Stream;

use crate::Result;

#[derive(Debug)]
pub struct ByteStream {
    chunk: Option<Vec<u8>>,
}

impl ByteStream {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        ByteStream { chunk: Some(bytes) }
    }
}

impl Stream for ByteStream {
    type Item = Result<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.chunk.take().map(Ok))
    }
}
