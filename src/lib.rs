use bytes::{buf::BufExt, Buf, Bytes};
use flume::{Receiver, Sender};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Result, Write},
    num::NonZeroU16,
    sync::Mutex,
};

static SWITCHBOARD: Lazy<Mutex<SwitchBoard>> =
    Lazy::new(|| Mutex::new(SwitchBoard(HashMap::default(), 1)));

struct SwitchBoard(HashMap<NonZeroU16, Sender<MemorySocket>>, u16);

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

    pub fn local_addr(&self) -> u16 {
        self.port.get()
    }

    pub fn incoming(&mut self) -> Incoming<'_> {
        Incoming { inner: self }
    }

    fn accept(&mut self) -> Option<MemorySocket> {
        self.incoming.iter().next()
    }
}

pub struct Incoming<'a> {
    inner: &'a mut MemoryListener,
}

impl<'a> Iterator for Incoming<'a> {
    type Item = Result<MemorySocket>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.accept().map(|socket| Ok(socket))
    }
}

pub struct MemorySocket {
    incoming: Receiver<Bytes>,
    outgoing: Sender<Bytes>,
    current_buffer: Option<Bytes>,
    seen_eof: bool,
}

impl MemorySocket {
    fn new(incoming: Receiver<Bytes>, outgoing: Sender<Bytes>) -> Self {
        Self {
            incoming,
            outgoing,
            current_buffer: None,
            seen_eof: false,
        }
    }

    pub fn new_pair() -> (Self, Self) {
        let (a_tx, a_rx) = flume::unbounded();
        let (b_tx, b_rx) = flume::unbounded();
        let a = Self::new(a_rx, b_tx);
        let b = Self::new(b_rx, a_tx);

        (a, b)
    }

    pub fn connet(port: u16) -> Result<MemorySocket> {
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
        match self.outgoing.send(Bytes::copy_from_slice(buf)) {
            Ok(()) => Ok(buf.len()),
            Err(_) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
