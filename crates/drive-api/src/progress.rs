//! Transfer telemetry. Reading all bytes is not proof of a committed sync operation.
use serde::Serialize;
use std::{
    io::{self, Read},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize)]
pub struct TransferProgress {
    pub path: String,
    pub upload: bool,
    pub bytes: u64,
    pub total: u64,
}
pub type ProgressSink = Arc<dyn Fn(TransferProgress) + Send + Sync>;

pub(crate) struct ProgressReader<R> {
    inner: R,
    event: TransferProgress,
    sink: Option<ProgressSink>,
    last: u64,
    step: u64,
}
impl<R> ProgressReader<R> {
    pub(crate) fn new(
        inner: R,
        path: &str,
        upload: bool,
        total: u64,
        sink: Option<ProgressSink>,
    ) -> Self {
        Self::new_at(inner, path, upload, total, sink, 0)
    }
    pub(crate) fn new_at(
        inner: R,
        path: &str,
        upload: bool,
        total: u64,
        sink: Option<ProgressSink>,
        offset: u64,
    ) -> Self {
        let event = TransferProgress {
            path: path.into(),
            upload,
            bytes: offset,
            total,
        };
        if let Some(sink) = &sink {
            sink(event.clone());
        }
        Self {
            inner,
            event,
            sink,
            last: offset,
            step: (total / 200).max(64 * 1024),
        }
    }
}
impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.event.bytes = self.event.bytes.saturating_add(count as u64);
        if self.event.bytes > self.last
            && (self.event.bytes.saturating_sub(self.last) >= self.step
                || self.event.bytes >= self.event.total)
        {
            if let Some(sink) = &self.sink {
                sink(self.event.clone());
            }
            self.last = self.event.bytes;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::sync::mpsc;
    #[test]
    fn progress_tracks_exact_bytes_without_claiming_operation_completion() {
        let (tx, rx) = mpsc::channel();
        let sink: ProgressSink = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        let bytes = vec![7_u8; 200_000];
        let mut reader = ProgressReader::new(
            io::Cursor::new(bytes.clone()),
            "synthetic.bin",
            true,
            bytes.len() as u64,
            Some(sink),
        );
        let mut received = Vec::new();
        io::copy(&mut reader, &mut received).unwrap();
        assert_eq!(received, bytes);
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events[0].bytes, 0);
        assert_eq!(events.last().unwrap().bytes, 200_000);
        assert!(events.windows(2).all(|pair| pair[0].bytes < pair[1].bytes));
        assert!(
            events
                .iter()
                .all(|event| event.upload && event.path == "synthetic.bin")
        );
    }
    #[test]
    fn failed_reader_does_not_emit_fictitious_bytes() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("synthetic failure"))
            }
        }
        let (tx, rx) = mpsc::channel();
        let mut reader = ProgressReader::new(
            Broken,
            "file",
            false,
            100,
            Some(Arc::new(move |event| {
                let _ = tx.send(event);
            })),
        );
        assert!(reader.read(&mut [0; 10]).is_err());
        assert_eq!(
            rx.try_iter().map(|event| event.bytes).collect::<Vec<_>>(),
            vec![0]
        );
    }
}
