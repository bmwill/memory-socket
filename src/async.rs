use crate::{Incoming, MemoryListener, MemorySocket};
use bytes::{buf::BufExt, Buf, Bytes};
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
    fn poll_accept(&mut self, context: &mut Context) -> Poll<Result<MemorySocket>> {
        match Pin::new(&mut self.incoming).poll_next(context) {
            Poll::Ready(Some(socket)) => Poll::Ready(Ok(socket)),
            // The stream will never terminate
            Poll::Ready(None) => unreachable!(),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<'a> Stream for Incoming<'a> {
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
    fn poll_write(self: Pin<&mut Self>, _context: &mut Context, buf: &[u8]) -> Poll<Result<usize>> {
        use flume::TrySendError;

        match self.outgoing.try_send(Bytes::copy_from_slice(buf)) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(TrySendError::Disconnected(_)) => Poll::Ready(Err(ErrorKind::BrokenPipe.into())),
            Err(TrySendError::Full(_)) => unreachable!(),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _context: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
