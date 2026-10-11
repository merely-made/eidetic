// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One bounded opaque object per asynchronous stream direction.
//!
//! The framing is a four-byte big-endian length, that many unchanged bytes,
//! and EOF. This is an authored carriage profile, not an application record
//! codec. The caller owns peer checks, record decoding, admission, and any
//! acknowledgement. Each call has one deadline covering all its I/O,
//! including the sender's shutdown or the receiver's EOF check.
//!
//! A refusal or timeout consumes part of the stream; discard that direction
//! rather than attempting to resynchronize it. A successful write shuts down
//! only the write half, leaving a duplex stream's reverse direction available
//! for an owner-generated acknowledgement.

use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Absolute ceiling for a configured object payload; the prefix is additional.
pub const MAX_OBJECT_STREAM_BYTES: u32 = 65_536;

/// Absolute ceiling for a configured full-transfer timeout.
pub const MAX_OBJECT_STREAM_TIMEOUT: Duration = Duration::from_secs(60);

/// Explicit resource bounds for one stream direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectStreamLimits {
    max_object_bytes: u32,
    transfer_timeout: Duration,
}

impl ObjectStreamLimits {
    /// Choose nonzero bounds within the small-object carriage ceiling.
    pub fn new(
        max_object_bytes: u32,
        transfer_timeout: Duration,
    ) -> Result<Self, ObjectStreamError> {
        if max_object_bytes == 0 || max_object_bytes > MAX_OBJECT_STREAM_BYTES {
            return Err(ObjectStreamError::InvalidLimits(
                "object ceiling must be between 1 and 65536 bytes",
            ));
        }
        if transfer_timeout.is_zero() || transfer_timeout > MAX_OBJECT_STREAM_TIMEOUT {
            return Err(ObjectStreamError::InvalidLimits(
                "transfer timeout must be greater than zero and at most 60 seconds",
            ));
        }
        Ok(Self {
            max_object_bytes,
            transfer_timeout,
        })
    }

    /// Largest accepted opaque payload, excluding its four-byte prefix.
    pub fn max_object_bytes(self) -> u32 {
        self.max_object_bytes
    }

    /// One deadline covering the complete call, including FIN/EOF.
    pub fn transfer_timeout(self) -> Duration {
        self.transfer_timeout
    }
}

/// A framing, resource, deadline, or stream error before application decoding.
#[derive(Debug, thiserror::Error)]
pub enum ObjectStreamError {
    #[error("invalid object stream limits: {0}")]
    InvalidLimits(&'static str),
    #[error("object of {declared} bytes exceeds the {max}-byte limit")]
    TooLarge { declared: u64, max: u32 },
    #[error("bytes follow the single framed object")]
    TrailingData,
    #[error("object stream transfer deadline expired")]
    Timeout,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Read one object and require EOF within the same deadline.
///
/// The untrusted prefix is checked before reserving a payload buffer or
/// reading any payload bytes. An empty payload is permitted here; its
/// application meaning remains the caller's decision. No result is returned
/// until the sender finishes the stream with no trailing bytes.
pub async fn read_object<S>(
    stream: &mut S,
    limits: ObjectStreamLimits,
) -> Result<Vec<u8>, ObjectStreamError>
where
    S: AsyncRead + Unpin,
{
    tokio::time::timeout(limits.transfer_timeout, async {
        let mut prefix = [0u8; 4];
        stream.read_exact(&mut prefix).await?;
        let length = u32::from_be_bytes(prefix);
        if length > limits.max_object_bytes {
            return Err(ObjectStreamError::TooLarge {
                declared: u64::from(length),
                max: limits.max_object_bytes,
            });
        }
        let mut object = vec![0u8; length as usize];
        stream.read_exact(&mut object).await?;
        let mut trailing = [0u8; 1];
        if stream.read(&mut trailing).await? != 0 {
            return Err(ObjectStreamError::TrailingData);
        }
        Ok(object)
    })
    .await
    .map_err(|_| ObjectStreamError::Timeout)?
}

/// Write the unchanged object and finish the write direction within one deadline.
///
/// An oversized local object is refused before writing its prefix. On QUIC
/// adapters, shutdown may wait for the peer to acknowledge final stream
/// bytes; that is transport delivery, not application admission. The peer must
/// read the object and EOF before returning its separate owner-level result.
pub async fn write_object<S>(
    stream: &mut S,
    object: &[u8],
    limits: ObjectStreamLimits,
) -> Result<(), ObjectStreamError>
where
    S: AsyncWrite + Unpin,
{
    if object.len() > limits.max_object_bytes as usize {
        return Err(ObjectStreamError::TooLarge {
            declared: object.len() as u64,
            max: limits.max_object_bytes,
        });
    }
    tokio::time::timeout(limits.transfer_timeout, async {
        stream
            .write_all(&(object.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(object).await?;
        stream.flush().await?;
        stream.shutdown().await?;
        Ok(())
    })
    .await
    .map_err(|_| ObjectStreamError::Timeout)?
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures_util::future::join;
    use tokio::io::ReadBuf;

    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    fn limits() -> ObjectStreamLimits {
        ObjectStreamLimits::new(16_384, Duration::from_secs(1)).unwrap()
    }

    #[test]
    fn exact_exchange_preserves_bytes_and_reverse_ack_direction() {
        runtime().block_on(async {
            let (mut sender, mut receiver) = tokio::io::duplex(8);
            let original = [0, 255, 128, 10, 42];
            let (written, recovered) = join(
                write_object(&mut sender, &original, limits()),
                read_object(&mut receiver, limits()),
            )
            .await;
            written.unwrap();
            assert_eq!(recovered.unwrap(), original);
            let (ack_written, ack) = join(
                write_object(&mut receiver, b"accepted", limits()),
                read_object(&mut sender, limits()),
            )
            .await;
            ack_written.unwrap();
            assert_eq!(ack.unwrap(), b"accepted");
            let mut empty = &0u32.to_be_bytes()[..];
            assert!(read_object(&mut empty, limits()).await.unwrap().is_empty());
        });
        assert_eq!(limits().max_object_bytes(), 16_384);
        assert_eq!(limits().transfer_timeout(), Duration::from_secs(1));
        for (max, timeout) in [
            (0, Duration::from_secs(1)),
            (65_537, Duration::from_secs(1)),
            (1, Duration::ZERO),
            (1, Duration::from_secs(61)),
        ] {
            assert!(matches!(
                ObjectStreamLimits::new(max, timeout),
                Err(ObjectStreamError::InvalidLimits(_))
            ));
        }
    }

    struct PrefixOnly {
        position: usize,
    }

    impl AsyncRead for PrefixOnly {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            assert!(self.position < 4, "oversized payload must never be polled");
            let prefix = u32::MAX.to_be_bytes();
            let count = buf.remaining().min(4 - self.position);
            buf.put_slice(&prefix[self.position..self.position + count]);
            self.position += count;
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn oversize_prefix_is_refused_before_payload_and_write_before_io() {
        runtime().block_on(async {
            let mut reader = PrefixOnly { position: 0 };
            assert!(matches!(
                read_object(&mut reader, limits()).await,
                Err(ObjectStreamError::TooLarge {
                    declared: 4_294_967_295,
                    max: 16_384
                })
            ));
            assert_eq!(reader.position, 4);
            let mut writer = ShutdownPending::default();
            let tiny = ObjectStreamLimits::new(1, Duration::from_secs(1)).unwrap();
            assert!(matches!(
                write_object(&mut writer, &[1, 2], tiny).await,
                Err(ObjectStreamError::TooLarge {
                    declared: 2,
                    max: 1
                })
            ));
            assert!(writer.bytes.is_empty());
            assert!(!writer.flushed);
        });
    }

    #[test]
    fn truncated_prefix_or_payload_and_trailing_bytes_are_refused() {
        runtime().block_on(async {
            for bytes in [&[0, 0][..], &[0, 0, 0, 3, 1][..]] {
                let mut reader = bytes;
                match read_object(&mut reader, limits()).await {
                    Err(ObjectStreamError::Io(error)) => {
                        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
                    },
                    other => panic!("expected truncated object, got {other:?}"),
                }
            }
            let mut reader = &[0, 0, 0, 1, 9, 8][..];
            assert!(matches!(
                read_object(&mut reader, limits()).await,
                Err(ObjectStreamError::TrailingData)
            ));
        });
    }

    #[derive(Default)]
    struct ShutdownPending {
        bytes: Vec<u8>,
        flushed: bool,
    }

    impl AsyncWrite for ShutdownPending {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.bytes.extend_from_slice(bytes);
            Poll::Ready(Ok(bytes.len()))
        }

        fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.flushed = true;
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    #[test]
    fn one_deadline_covers_progressive_reads_eof_and_write_shutdown() {
        runtime().block_on(async {
            let (mut sender, mut receiver) = tokio::io::duplex(16);
            let budget = ObjectStreamLimits::new(16, Duration::from_millis(50)).unwrap();
            let (_, recovered) = join(
                async {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    sender.write_all(&1u32.to_be_bytes()).await.unwrap();
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    sender.write_all(&[7]).await.unwrap();
                    sender.shutdown().await.unwrap();
                },
                read_object(&mut receiver, budget),
            )
            .await;
            assert!(matches!(recovered, Err(ObjectStreamError::Timeout)));

            let (mut sender, mut receiver) = tokio::io::duplex(16);
            sender.write_all(&[0, 0, 0, 1, 7]).await.unwrap();
            let short = ObjectStreamLimits::new(16, Duration::from_millis(10)).unwrap();
            assert!(matches!(
                read_object(&mut receiver, short).await,
                Err(ObjectStreamError::Timeout)
            ));

            let mut writer = ShutdownPending::default();
            assert!(matches!(
                write_object(&mut writer, &[7], short).await,
                Err(ObjectStreamError::Timeout)
            ));
            assert_eq!(writer.bytes, [0, 0, 0, 1, 7]);
            assert!(writer.flushed);
        });
    }
}
