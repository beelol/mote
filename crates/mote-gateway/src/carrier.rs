use anyhow::{bail, ensure, Result};
use futures_util::{SinkExt, StreamExt};
use std::{net::SocketAddr, time::Duration};
use tokio::{net::TcpStream, runtime::Runtime};
use tokio_tungstenite::{
    accept_hdr_async_with_config, client_async_with_config,
    tungstenite::{handshake::server::Request, protocol::WebSocketConfig, Message},
    WebSocketStream,
};

pub(crate) struct Socket {
    runtime: Runtime,
    stream: WebSocketStream<TcpStream>,
    write_timeout: Duration,
}

fn runtime() -> Result<Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

fn limits() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(65_535))
        .max_frame_size(Some(65_535))
}

impl Socket {
    pub(crate) fn connect(address: SocketAddr, pairing: bool, timeout: Duration) -> Result<Self> {
        let runtime = runtime()?;
        let path = if pairing { "pair" } else { "session" };
        let stream = runtime.block_on(async {
            tokio::time::timeout(timeout, async {
                let tcp = TcpStream::connect(address).await?;
                let (stream, _) =
                    client_async_with_config(format!("ws://{address}/{path}"), tcp, Some(limits()))
                        .await?;
                Ok::<_, anyhow::Error>(stream)
            })
            .await
        })??;
        Ok(Self {
            runtime,
            stream,
            write_timeout: timeout,
        })
    }

    pub(crate) fn accept(tcp: std::net::TcpStream, timeout: Duration) -> Result<(Self, bool)> {
        tcp.set_nonblocking(true)?;
        let runtime = runtime()?;
        let (stream, pairing) = runtime.block_on(async {
            let tcp = TcpStream::from_std(tcp)?;
            let mut pairing = false;
            let callback = |request: &Request, response| {
                match request.uri().path_and_query().map(|path| path.as_str()) {
                    Some("/pair") => pairing = true,
                    Some("/session") => {}
                    _ => {
                        let mut error =
                            tokio_tungstenite::tungstenite::handshake::server::ErrorResponse::new(
                                Some("unknown gateway endpoint".into()),
                            );
                        *error.status_mut() =
                            tokio_tungstenite::tungstenite::http::StatusCode::BAD_REQUEST;
                        return Err(error);
                    }
                }
                Ok(response)
            };
            let stream = tokio::time::timeout(
                timeout,
                accept_hdr_async_with_config(tcp, callback, Some(limits())),
            )
            .await??;
            Ok::<_, anyhow::Error>((stream, pairing))
        })?;
        Ok((
            Self {
                runtime,
                stream,
                write_timeout: timeout,
            },
            pairing,
        ))
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= 65_535,
            "invalid Noise message length"
        );
        self.runtime.block_on(async {
            tokio::time::timeout(
                self.write_timeout,
                self.stream.send(Message::Binary(bytes.to_vec().into())),
            )
            .await??;
            Ok(())
        })
    }

    pub(crate) fn read(&mut self, max: usize, timeout: Option<Duration>) -> Result<Vec<u8>> {
        self.runtime.block_on(async {
            let read = async {
                loop {
                    match self
                        .stream
                        .next()
                        .await
                        .transpose()?
                        .ok_or_else(|| anyhow::anyhow!("gateway disconnected"))?
                    {
                        Message::Binary(bytes) => {
                            ensure!(
                                !bytes.is_empty() && bytes.len() <= max,
                                "invalid Noise message length"
                            );
                            return Ok(bytes.to_vec());
                        }
                        Message::Ping(_) => self.stream.flush().await?,
                        Message::Pong(_) => {}
                        _ => bail!("gateway requires binary Noise messages"),
                    }
                }
            };
            match timeout {
                Some(timeout) => tokio::time::timeout(timeout, read).await?,
                None => read.await,
            }
        })
    }
}
