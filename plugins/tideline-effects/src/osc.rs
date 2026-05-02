use anyhow::{anyhow, Context, Result};
use rosc::{OscMessage, OscPacket, OscType};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

pub struct OscClient {
    sock: UdpSocket,
    target: SocketAddr,
}

impl OscClient {
    pub async fn connect(target: SocketAddr) -> Result<Self> {
        let sock = UdpSocket::bind("127.0.0.1:0")
            .await
            .context("bind osc client socket")?;
        Ok(Self { sock, target })
    }

    pub async fn send(&self, addr: &str, args: Vec<OscType>) -> Result<()> {
        let pkt = OscPacket::Message(OscMessage {
            addr: addr.to_string(),
            args,
        });
        let bytes = rosc::encoder::encode(&pkt).context("encode osc packet")?;
        self.sock
            .send_to(&bytes, self.target)
            .await
            .context("send osc packet")?;
        Ok(())
    }

    /// Receive a single reply with a deadline. Returns the parsed message or error on timeout.
    pub async fn recv(&self, dur: Duration) -> Result<OscMessage> {
        let mut buf = vec![0u8; 4096];
        let (n, _src) = timeout(dur, self.sock.recv_from(&mut buf))
            .await
            .map_err(|_| anyhow!("osc recv timeout after {:?}", dur))??;
        let pkt = rosc::decoder::decode_udp(&buf[..n])
            .context("decode osc packet")?
            .1;
        match pkt {
            OscPacket::Message(m) => Ok(m),
            OscPacket::Bundle(_) => Err(anyhow!("expected osc message, got bundle")),
        }
    }
}
