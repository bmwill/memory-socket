use memory_socket::{MemoryListener, MemorySocket};
use std::io::{Read, Result, Write};

//
// MemoryListener Tests
//

#[test]
fn listener_bind() -> Result<()> {
    let listener = MemoryListener::bind(42)?;
    assert_eq!(listener.local_addr(), 42);

    Ok(())
}

#[test]
fn simple_connect() -> Result<()> {
    let listener = MemoryListener::bind(10)?;

    let mut dialer = MemorySocket::connect(10)?;
    let mut listener_socket = listener.incoming().next().unwrap()?;

    dialer.write_all(b"foo")?;
    dialer.flush()?;

    let mut buf = [0; 3];
    listener_socket.read_exact(&mut buf)?;
    assert_eq!(&buf, b"foo");

    Ok(())
}

#[test]
fn listen_on_port_zero() -> Result<()> {
    let listener = MemoryListener::bind(0)?;
    let listener_addr = listener.local_addr();

    let mut dialer = MemorySocket::connect(listener_addr)?;
    let mut listener_socket = listener.incoming().next().unwrap()?;

    dialer.write_all(b"foo")?;
    dialer.flush()?;

    let mut buf = [0; 3];
    listener_socket.read_exact(&mut buf)?;
    assert_eq!(&buf, b"foo");

    listener_socket.write_all(b"bar")?;
    listener_socket.flush()?;

    let mut buf = [0; 3];
    dialer.read_exact(&mut buf)?;
    assert_eq!(&buf, b"bar");

    Ok(())
}

#[test]
fn listener_correctly_frees_port_on_drop() -> Result<()> {
    fn connect_on_port(port: u16) -> Result<()> {
        let listener = MemoryListener::bind(port)?;
        let mut dialer = MemorySocket::connect(port)?;
        let mut listener_socket = listener.incoming().next().unwrap()?;

        dialer.write_all(b"foo")?;
        dialer.flush()?;

        let mut buf = [0; 3];
        listener_socket.read_exact(&mut buf)?;
        assert_eq!(&buf, b"foo");

        Ok(())
    }

    connect_on_port(9)?;
    connect_on_port(9)?;

    Ok(())
}

//
// MemorySocket Tests
//

#[test]
fn simple_write_read() -> Result<()> {
    let (mut a, mut b) = MemorySocket::new_pair();

    a.write_all(b"hello world")?;
    a.flush()?;
    drop(a);

    let mut v = Vec::new();
    b.read_to_end(&mut v)?;
    assert_eq!(v, b"hello world");

    Ok(())
}

#[test]
fn partial_read() -> Result<()> {
    let (mut a, mut b) = MemorySocket::new_pair();

    a.write_all(b"foobar")?;
    a.flush()?;

    let mut buf = [0; 3];
    b.read_exact(&mut buf)?;
    assert_eq!(&buf, b"foo");
    b.read_exact(&mut buf)?;
    assert_eq!(&buf, b"bar");

    Ok(())
}

#[test]
fn partial_read_write_both_sides() -> Result<()> {
    let (mut a, mut b) = MemorySocket::new_pair();

    a.write_all(b"foobar")?;
    a.flush()?;
    b.write_all(b"stormlight")?;
    b.flush()?;

    let mut buf_a = [0; 5];
    let mut buf_b = [0; 3];
    a.read_exact(&mut buf_a)?;
    assert_eq!(&buf_a, b"storm");
    b.read_exact(&mut buf_b)?;
    assert_eq!(&buf_b, b"foo");

    a.read_exact(&mut buf_a)?;
    assert_eq!(&buf_a, b"light");
    b.read_exact(&mut buf_b)?;
    assert_eq!(&buf_b, b"bar");

    Ok(())
}

#[test]
fn many_small_writes() -> Result<()> {
    let (mut a, mut b) = MemorySocket::new_pair();

    a.write_all(b"words")?;
    a.write_all(b" ")?;
    a.write_all(b"of")?;
    a.write_all(b" ")?;
    a.write_all(b"radiance")?;
    a.flush()?;
    drop(a);

    let mut buf = [0; 17];
    b.read_exact(&mut buf)?;
    assert_eq!(&buf, b"words of radiance");

    Ok(())
}

#[test]
fn read_zero_bytes() -> Result<()> {
    let (mut a, mut b) = MemorySocket::new_pair();

    a.write_all(b"way of kings")?;
    a.flush()?;

    let mut buf = [0; 12];
    b.read_exact(&mut buf[0..0])?;
    assert_eq!(buf, [0; 12]);
    b.read_exact(&mut buf)?;
    assert_eq!(&buf, b"way of kings");

    Ok(())
}

#[test]
fn read_bytes_with_large_buffer() -> Result<()> {
    let (mut a, mut b) = MemorySocket::new_pair();

    a.write_all(b"way of kings")?;
    a.flush()?;

    let mut buf = [0; 20];
    let bytes_read = b.read(&mut buf)?;
    assert_eq!(bytes_read, 12);
    assert_eq!(&buf[0..12], b"way of kings");

    Ok(())
}
