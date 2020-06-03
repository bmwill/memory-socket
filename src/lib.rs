//! Provides an in-memory socket abstraction.
//!
//! The `memory-socket` crate provides the [`MemoryListener`] and [`MemorySocket`] types which can
//! be thought of as in-memory versions of the standard library `TcpListener` and `TcpStream`
//! types.
//!
//! ## Feature flags
//!
//! - `async`: Adds async support for [`MemorySocket`] and [`MemoryListener`]
//!
//! [`MemoryListener`]: struct.MemoryListener.html
//! [`MemorySocket`]: struct.MemorySocket.html

use bytes::{buf::BufExt, Buf, Bytes, BytesMut};
use flume::{Receiver, Sender};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Result, Write},
    num::NonZeroU16,
    sync::Mutex,
};

#[cfg(feature = "async")]
mod r#async;

#[cfg(feature = "async")]
pub use r#async::IncomingStream;

static SWITCHBOARD: Lazy<Mutex<SwitchBoard>> =
    Lazy::new(|| Mutex::new(SwitchBoard(HashMap::default(), 1)));

struct SwitchBoard(HashMap<NonZeroU16, Sender<MemorySocket>>, u16);

/// An in-memory socket server, listening for connections.
///
/// After creating a `MemoryListener` by [`bind`]ing it to a socket address, it listens
/// for incoming connections. These can be accepted by calling [`accept`] or by iterating over
/// iterating over the [`Incoming`] iterator returned by [`incoming`][`MemoryListener::incoming`].
///
/// The socket will be closed when the value is dropped.
///
/// [`accept`]: #method.accept
/// [`bind`]: #method.bind
/// [`Incoming`]: struct.Incoming.html
/// [`MemoryListener::incoming`]: #method.incoming
///
/// # Examples
///
/// ```no_run
/// use std::io::{Read, Result, Write};
///
/// use memory_socket::{MemoryListener, MemorySocket};
///
/// fn write_stormlight(mut stream: MemorySocket) -> Result<()> {
///     let msg = b"The most important step a person can take is always the next one.";
///     stream.write_all(msg)?;
///     stream.flush()
/// }
///
/// fn main() -> Result<()> {
///     let mut listener = MemoryListener::bind(16)?;
///
///     // accept connections and process them serially
///     for stream in listener.incoming() {
///         write_stormlight(stream?)?;
///     }
///     Ok(())
/// }
/// ```
pub struct MemoryListener {
    incoming: Receiver<MemorySocket>,
    port: NonZeroU16,
}

impl Drop for MemoryListener {
    fn drop(&mut self) {
        let mut switchboard = (&*SWITCHBOARD).lock().unwrap();
        // Remove the Sending side of the channel in the switchboard when
        // MemoryListener is dropped
        switchboard.0.remove(&self.port);
    }
}

impl MemoryListener {
    /// Creates a new `MemoryListener` which will be bound to the specified
    /// port.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Binding with a port number of `0` will request that a port be assigned
    /// to this listener. The port allocated can be queried via the
    /// [`local_addr`] method.
    ///
    /// [`local_addr`]: #method.local_addr
    ///
    /// # Examples
    ///
    /// Create a MemoryListener bound to port 16:
    ///
    /// ```no_run
    /// use memory_socket::MemoryListener;
    ///
    /// # fn main () -> ::std::io::Result<()> {
    /// let listener = MemoryListener::bind(16)?;
    /// # Ok(())}
    /// ```
    pub fn bind(port: u16) -> Result<Self> {
        let mut switchboard = (&*SWITCHBOARD).lock().unwrap();

        // Get the port we should bind to.  If 0 was given, use a random port
        let port = if let Some(port) = NonZeroU16::new(port) {
            if switchboard.0.contains_key(&port) {
                return Err(ErrorKind::AddrInUse.into());
            }

            port
        } else {
            loop {
                let port = NonZeroU16::new(switchboard.1).unwrap_or_else(|| unreachable!());

                // The switchboard is full and all ports are in use
                if switchboard.0.len() == (std::u16::MAX - 1) as usize {
                    return Err(ErrorKind::AddrInUse.into());
                }

                // Instead of overflowing to 0, resume searching at port 1 since port 0 isn't a
                // valid port to bind to.
                if switchboard.1 == std::u16::MAX {
                    switchboard.1 = 1;
                } else {
                    switchboard.1 += 1;
                }

                if !switchboard.0.contains_key(&port) {
                    break port;
                }
            }
        };

        let (sender, receiver) = flume::unbounded();
        switchboard.0.insert(port, sender);

        Ok(Self {
            incoming: receiver,
            port,
        })
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port `0` to figure out
    /// which port was actually bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use memory_socket::MemoryListener;
    ///
    /// # fn main () -> ::std::io::Result<()> {
    /// let listener = MemoryListener::bind(16)?;
    ///
    /// assert_eq!(listener.local_addr(), 16);
    /// # Ok(())}
    /// ```
    pub fn local_addr(&self) -> u16 {
        self.port.get()
    }

    /// Returns an iterator over the connections being received on this
    /// listener.
    ///
    /// The returned iterator will never return `None`. Iterating over
    /// it is equivalent to calling [`accept`] in a loop.
    ///
    /// [`accept`]: #method.accept
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use memory_socket::MemoryListener;
    /// use std::io::{Read, Write};
    ///
    /// let mut listener = MemoryListener::bind(80).unwrap();
    ///
    /// for stream in listener.incoming() {
    ///     match stream {
    ///         Ok(stream) => {
    ///             println!("new client!");
    ///         }
    ///         Err(e) => { /* connection failed */ }
    ///     }
    /// }
    /// ```
    pub fn incoming(&self) -> Incoming<'_> {
        Incoming { inner: self }
    }

    /// Accept a new incoming connection from this listener.
    ///
    /// This function will block the calling thread until a new connection
    /// is established. When established, the corresponding [`MemorySocket`]
    /// will be returned.
    ///
    /// [`MemorySocket`]: struct.MemorySocket.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::TcpListener;
    /// use memory_socket::MemoryListener;
    ///
    /// let mut listener = MemoryListener::bind(8080).unwrap();
    /// match listener.accept() {
    ///     Ok(_socket) => println!("new client!"),
    ///     Err(e) => println!("couldn't get client: {:?}", e),
    /// }
    /// ```
    pub fn accept(&self) -> Result<MemorySocket> {
        self.incoming.iter().next().ok_or_else(|| unreachable!())
    }
}

/// An iterator that infinitely [`accept`]s connections on a [`MemoryListener`].
///
/// This `struct` is created by the [`incoming`] method on [`MemoryListener`].
/// See its documentation for more info.
///
/// [`accept`]: struct.MemoryListener.html#method.accept
/// [`incoming`]: struct.MemoryListener.html#method.incoming
/// [`MemoryListener`]: struct.MemoryListener.html
pub struct Incoming<'a> {
    inner: &'a MemoryListener,
}

impl<'a> Iterator for Incoming<'a> {
    type Item = Result<MemorySocket>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.inner.accept())
    }
}

/// An in-memory stream between two local sockets.
///
/// A `MemorySocket` can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [accepting] a connection from a [listener].
/// It can be read or written to using the `Read` and `Write` traits.
///
/// # Examples
///
/// ```
/// use std::io::{Read, Result, Write};
/// use memory_socket::MemorySocket;
///
/// # fn main() -> Result<()> {
/// let (mut socket_a, mut socket_b) = MemorySocket::new_pair();
///
/// socket_a.write_all(b"stormlight")?;
/// socket_a.flush()?;
///
/// let mut buf = [0; 10];
/// socket_b.read_exact(&mut buf)?;
/// assert_eq!(&buf, b"stormlight");
///
/// # Ok(())}
/// ```
///
/// [`connect`]: struct.MemorySocket.html#method.connect
/// [accepting]: struct.MemoryListener.html#method.accept
/// [listener]: struct.MemoryListener.html
pub struct MemorySocket {
    incoming: Receiver<Bytes>,
    outgoing: Sender<Bytes>,
    write_buffer: BytesMut,
    current_buffer: Option<Bytes>,
    seen_eof: bool,
}

impl MemorySocket {
    fn new(incoming: Receiver<Bytes>, outgoing: Sender<Bytes>) -> Self {
        Self {
            incoming,
            outgoing,
            write_buffer: BytesMut::new(),
            current_buffer: None,
            seen_eof: false,
        }
    }

    /// Construct both sides of an in-memory socket.
    ///
    /// # Examples
    ///
    /// ```
    /// use memory_socket::MemorySocket;
    ///
    /// let (socket_a, socket_b) = MemorySocket::new_pair();
    /// ```
    pub fn new_pair() -> (Self, Self) {
        let (a_tx, a_rx) = flume::unbounded();
        let (b_tx, b_rx) = flume::unbounded();
        let a = Self::new(a_rx, b_tx);
        let b = Self::new(b_rx, a_tx);

        (a, b)
    }

    /// Create a new in-memory Socket connected to the specified port.
    ///
    /// This function will create a new MemorySocket socket and attempt to connect it to
    /// the `port` provided.
    ///
    /// # Examples
    ///
    /// ```
    /// use memory_socket::MemorySocket;
    ///
    /// # fn main () -> ::std::io::Result<()> {
    /// # let _listener = memory_socket::MemoryListener::bind(16)?;
    /// let socket = MemorySocket::connect(16)?;
    /// # Ok(())}
    /// ```
    pub fn connect(port: u16) -> Result<MemorySocket> {
        let mut switchboard = (&*SWITCHBOARD).lock().unwrap();

        // Find port to connect to
        let port = NonZeroU16::new(port).ok_or_else(|| ErrorKind::AddrNotAvailable)?;

        let sender = switchboard
            .0
            .get_mut(&port)
            .ok_or_else(|| ErrorKind::AddrNotAvailable)?;

        let (socket_a, socket_b) = Self::new_pair();

        // Send the socket to the listener
        sender
            .send(socket_a)
            .map_err(|_| ErrorKind::AddrNotAvailable)?;

        Ok(socket_b)
    }
}

impl Read for MemorySocket {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut bytes_read = 0;

        loop {
            // If we've already filled up the buffer then we can return
            if bytes_read == buf.len() {
                return Ok(bytes_read);
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
                        return Ok(bytes_read);
                    }

                    self.current_buffer = match self.incoming.recv() {
                        Ok(buf) => Some(buf),

                        // The remote side hung up, if this is the first time we've seen EOF then
                        // we should return `Ok(0)` otherwise an UnexpectedEof Error
                        Err(_) => {
                            if self.seen_eof {
                                return Err(ErrorKind::UnexpectedEof.into());
                            } else {
                                self.seen_eof = true;
                                return Ok(0);
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Write for MemorySocket {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.write_buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        if !self.write_buffer.is_empty() {
            self.outgoing
                .send(self.write_buffer.split().freeze())
                .map_err(|_| ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        }
    }
}
