use crate::{MemoryListener, MemorySocket};
use bytes::{buf::BufExt, Buf};
use futures::{
    io::{AsyncRead, AsyncWrite},
    ready,
    stream::{FusedStream, Stream},
};
use std::{
    io::{ErrorKind, Result},
    pin::Pin,
    task::{Context, Poll},
};

impl MemoryListener {
    /// Returns a stream over the connections being received on this
    /// listener.
    ///
    /// The returned stream will never return `None`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use futures::prelude::*;
    /// use memory_socket::MemoryListener;
    ///
    /// # async fn work () -> ::std::io::Result<()> {
    /// let mut listener = MemoryListener::bind("192.51.100.2:60".parse().unwrap()).unwrap();
    /// let mut incoming = listener.incoming_stream();
    ///
    /// while let Some(stream) = incoming.next().await {
    ///     match stream {
    ///         Ok(stream) => {
    ///             println!("new client!");
    ///         }
    ///         Err(e) => { /* connection failed */ }
    ///     }
    /// }
    /// # Ok(())}
    /// ```
    pub fn incoming_stream(&mut self) -> IncomingStream<'_> {
        IncomingStream { inner: self }
    }

    fn poll_accept(&mut self, context: &mut Context) -> Poll<Result<MemorySocket>> {
        match Pin::new(&mut self.incoming).poll_next(context) {
            Poll::Ready(Some(socket)) => Poll::Ready(Ok(socket)),
            // The stream will never terminate
            Poll::Ready(None) => unreachable!(),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// A Stream that infinitely accepts connections on a [`MemoryListener`].
///
/// This `struct` is created by the [`incoming_stream`] method on [`MemoryListener`].
/// See its documentation for more info.
///
/// [`incoming_stream`]: struct.MemoryListener.html#method.incoming_stream
/// [`MemoryListener`]: struct.MemoryListener.html
pub struct IncomingStream<'a> {
    inner: &'a mut MemoryListener,
}

impl<'a> Stream for IncomingStream<'a> {
    type Item = Result<MemorySocket>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Option<Self::Item>> {
        let socket = ready!(self.inner.poll_accept(context)?);
        Poll::Ready(Some(Ok(socket)))
    }
}

impl AsyncRead for MemorySocket {
    fn poll_read(
        mut self: Pin<&mut Self>,
        mut context: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        if self.incoming.is_terminated() {
            if self.seen_eof {
                return Poll::Ready(Err(ErrorKind::UnexpectedEof.into()));
            } else {
                self.seen_eof = true;
                return Poll::Ready(Ok(0));
            }
        }

        let mut bytes_read = 0;

        loop {
            // If we've already filled up the buffer then we can return
            if bytes_read == buf.len() {
                return Poll::Ready(Ok(bytes_read));
            }

            match self.current_buffer {
                // We still have data to copy to `buf`
                Some(ref mut current_buffer) if current_buffer.has_remaining() => {
                    let bytes_to_read =
                        ::std::cmp::min(buf.len() - bytes_read, current_buffer.remaining());
                    debug_assert!(bytes_to_read > 0);

                    current_buffer
                        .take(bytes_to_read)
                        .copy_to_slice(&mut buf[bytes_read..(bytes_read + bytes_to_read)]);
                    bytes_read += bytes_to_read;
                }

                // Either we've exhausted our current buffer or we don't have one
                _ => {
                    // If we've read anything up to this point return the bytes read
                    if bytes_read > 0 {
                        return Poll::Ready(Ok(bytes_read));
                    }

                    self.current_buffer = {
                        match Pin::new(&mut self.incoming).poll_next(&mut context) {
                            Poll::Pending => return Poll::Pending,
                            Poll::Ready(Some(buf)) => Some(buf),
                            Poll::Ready(None) => return Poll::Ready(Ok(bytes_read)),
                        }
                    };
                }
            }
        }
    }
}

impl AsyncWrite for MemorySocket {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _context: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize>> {
        self.write_buffer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _context: &mut Context) -> Poll<Result<()>> {
        use flume::TrySendError;

        if !self.write_buffer.is_empty() {
            let buffer = self.write_buffer.split().freeze();
            match self.outgoing.try_send(buffer) {
                Ok(()) => Poll::Ready(Ok(())),
                Err(TrySendError::Disconnected(_)) => {
                    Poll::Ready(Err(ErrorKind::BrokenPipe.into()))
                }
                Err(TrySendError::Full(_)) => unreachable!(),
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_close(self: Pin<&mut Self>, _context: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
