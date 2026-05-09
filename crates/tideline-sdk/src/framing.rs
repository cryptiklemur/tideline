use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    w.write_all(header.as_bytes()).await?;
    w.write_all(payload).await?;
    w.flush().await
}

pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut header = Vec::with_capacity(64);
    let mut byte = [0u8; 1];
    loop {
        let n = r.read(&mut byte).await?;
        if n == 0 {
            return if header.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("eof mid-header (got {})", sample_bytes(&header)),
                ))
            };
        }
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("header too large (first 128: {})", sample_bytes(&header)),
            ));
        }
    }
    let header_str = std::str::from_utf8(&header).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("non-utf8 header ({})", sample_bytes(&header)),
        )
    })?;
    let mut len: Option<usize> = None;
    for line in header_str.split("\r\n") {
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once(": ").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("bad header line ({})", sample_bytes(line.as_bytes())),
            )
        })?;
        if k.eq_ignore_ascii_case("Content-Length") {
            len = Some(v.trim().parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, format!("bad length: {v:?}"))
            })?);
        }
    }
    let len = len.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("no content-length (header: {})", sample_bytes(&header)),
        )
    })?;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

fn sample_bytes(b: &[u8]) -> String {
    let n = b.len().min(128);
    String::from_utf8_lossy(&b[..n]).escape_debug().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn round_trip() {
        let (mut a, mut b) = duplex(4096);
        let payload = br#"{"jsonrpc":"2.0","method":"x"}"#.to_vec();
        write_frame(&mut a, &payload).await.unwrap();
        let got = read_frame(&mut b).await.unwrap().unwrap();
        assert_eq!(got, payload);
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let (a, mut b) = duplex(4096);
        drop(a);
        let got = read_frame(&mut b).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn rejects_oversize_header() {
        let (mut a, mut b) = duplex(8192);
        let mut header = Vec::new();
        for _ in 0..200 {
            header.extend_from_slice(b"X-Junk: aaaaaaaaaaaaaaaaaaaaaa\r\n");
        }
        a.write_all(&header).await.unwrap();
        let err = read_frame(&mut b).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
