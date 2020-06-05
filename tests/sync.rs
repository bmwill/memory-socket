use memory_socket::{MemoryListener, MemorySocket};
use std::{
    io::{Read, Result, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

//
// MemoryListener Tests
//

#[test]
fn listener_bind() -> Result<()> {
    let listener = MemoryListener::bind("192.51.100.2:42").expect("Should listen on valid address");
    let expected = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 51, 100, 2)), 42);
    let actual = listener
        .local_addr()
        .expect("Socket should have a local address");
    assert_eq!(actual, expected);

    Ok(())
}

#[test]
fn bind_unspecified() {
    // Current implementation does not know how to handle unspecified address
    let listener_result = MemoryListener::bind("0.0.0.0:0");
    assert!(listener_result.is_err());
}

#[test]
fn simple_connect() -> Result<()> {
    let listener = MemoryListener::bind("192.51.100.2:1337")?;

    let mut dialer = MemorySocket::connect("192.51.100.2:1337")?;
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
    let listener = MemoryListener::bind("192.51.100.3:0").expect("Should listen on port 0");
    let listener_addr = listener.local_addr().expect("Should have a local address");
    assert_eq!(
        listener_addr.ip(),
        IpAddr::V4(Ipv4Addr::new(192, 51, 100, 3))
    );
    assert_ne!(listener_addr.port(), 0);

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
    fn connect_to(address: SocketAddr) -> Result<()> {
        let listener = MemoryListener::bind(address)?;
        let mut dialer = MemorySocket::connect(address)?;
        let mut listener_socket = listener.incoming().next().unwrap()?;

        dialer.write_all(b"foo")?;
        dialer.flush()?;

        let mut buf = [0; 3];
        listener_socket.read_exact(&mut buf)?;
        assert_eq!(&buf, b"foo");

        Ok(())
    }

    connect_to("192.51.100.3:9".parse().unwrap())?;
    connect_to("192.51.100.3:9".parse().unwrap())?;

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
