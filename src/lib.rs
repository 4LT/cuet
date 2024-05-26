use io::{ErrorKind, Read, Seek, SeekFrom};
use std::io;

const RIFF_HEAD_SZ: u64 = 8;

#[derive(Debug)]
pub enum Error {
    Wave(String),
    Io(io::Error),
}

impl Error {
    fn wave<S: ToString>(s: S) -> Self {
        Self::Wave(s.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(
        &self,
        formatter: &mut std::fmt::Formatter<'_>,
    ) -> Result<(), std::fmt::Error> {
        match self {
            Self::Wave(s) => writeln!(formatter, "Error: Wave: {}", s)?,
            Self::Io(e) => writeln!(formatter, "Error: IO: {}", e)?,
        }

        Ok(())
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl std::error::Error for Error {}

#[derive(Debug)]
pub struct WaveCursor<'a, Cursor: Read + Seek> {
    head: [u8; RIFF_HEAD_SZ as usize],
    end: u64,
    base_cursor: &'a mut Cursor,
    offset: u64,
    position: u64,
}

impl<'a, Cursor: Read + Seek> WaveCursor<'a, Cursor> {
    fn new(riff_sz: u32, base_cursor: &'a mut Cursor) -> io::Result<Self> {
        let offset = base_cursor.stream_position()?;

        let mut head = [0u8; RIFF_HEAD_SZ as usize];

        (&b"RIFF"[..]).read_exact(&mut head[..4]).and_then(|_| {
            (&riff_sz.to_le_bytes()[..]).read_exact(&mut head[4..])
        })?;

        let end = u64::from(riff_sz) + RIFF_HEAD_SZ;

        Ok(Self {
            head,
            end,
            base_cursor,
            offset,
            position: 0,
        })
    }
}

impl<'a, Cursor: Read + Seek> Seek for WaveCursor<'a, Cursor> {
    fn seek(&mut self, seek_from: SeekFrom) -> io::Result<u64> {
        let seek_err = io::Error::new(
            ErrorKind::InvalidInput,
            "attempted negative or overflowing seek",
        );

        match seek_from {
            SeekFrom::Start(new_pos) => {
                self.position = new_pos;
            }
            SeekFrom::Current(offset) => {
                self.position =
                    self.position.checked_add_signed(offset).ok_or(seek_err)?;
            }
            SeekFrom::End(end_offset) => {
                self.position =
                    self.end.checked_add_signed(end_offset).ok_or(seek_err)?;
            }
        }

        Ok(self.position)
    }
}

impl<'a, Cursor: Read + Seek> Read for WaveCursor<'a, Cursor> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read_err = || {
            io::Error::new(
                ErrorKind::InvalidInput,
                "attempted to read beyond bounds",
            )
        };

        let read_start = self.position.max(RIFF_HEAD_SZ);

        let head_bytes =
            usize::try_from(RIFF_HEAD_SZ.saturating_sub(self.position))
                .unwrap()
                .min(buf.len());

        let max_read_ct = buf.len().min(
            usize::try_from(self.end.saturating_sub(self.position))
                .map_err(|_| read_err())?,
        );

        let head_start =
            usize::try_from(self.position.min(RIFF_HEAD_SZ)).unwrap();

        let head_end = head_start + head_bytes;
        let bytes_to_read = buf.len().saturating_sub(head_bytes);
        let read_end = head_end.checked_add(bytes_to_read).ok_or(read_err())?;

        buf[..head_bytes].copy_from_slice(&self.head[head_start..head_end]);

        let read_bytes = read_start
            .checked_add(self.offset)
            .ok_or(read_err())
            .and_then(|start| self.base_cursor.seek(SeekFrom::Start(start)))
            .and_then(|_| {
                self.base_cursor.read(&mut buf[head_bytes..max_read_ct])
            })?;

        let total_bytes =
            read_bytes.checked_add(head_bytes).ok_or(read_err())?;

        self.position = self
            .position
            .checked_add(total_bytes.try_into().map_err(|_| read_err())?)
            .ok_or(read_err())?;

        Ok(total_bytes)
    }
}

#[derive(Debug)]
pub struct SplitCursor<'a, Cursor: Read + Seek> {
    base_cursor: &'a mut Cursor,
    wave_start: u64,
    wave_sz: u32,
    appendix_start: u64,
}

impl<'a, Cursor: Read + Seek> SplitCursor<'a, Cursor> {
    pub fn new(cursor: &'a mut Cursor) -> Result<Self, Error> {
        let mut riff_id = [0u8; 4];
        let mut riff_sz = [0u8; 4]; // unused
        let mut wave_id = [0u8; 4];
        let mut fmt_id = [0u8; 4];
        let mut fmt_sz = [0u8; 4];
        let mut data_id = [0u8; 4];
        let mut data_sz = [0u8; 4];

        let wave_start = cursor.stream_position().map_err(Error::Io)?;

        cursor
            .read_exact(&mut riff_id)
            .and_then(|_| cursor.read_exact(&mut riff_sz))
            .and_then(|_| cursor.read_exact(&mut wave_id))
            .and_then(|_| cursor.read_exact(&mut fmt_id))
            .and_then(|_| cursor.read_exact(&mut fmt_sz))
            .map_err(Error::Io)?;

        if riff_id != *b"RIFF" || wave_id != *b"WAVE" {
            return Err(Error::wave("Not a WAVE file"));
        }

        if fmt_id != *b"fmt\0" {
            return Err(Error::wave("Malformed WAVE file: Missing \"fmt\""));
        }

        let fmt_sz = u32::from_le_bytes(fmt_sz);

        cursor
            .seek(SeekFrom::Current(fmt_sz.into()))
            .map_err(Error::Io)?;

        cursor
            .read_exact(&mut data_id)
            .and_then(|_| cursor.read_exact(&mut data_sz))
            .map_err(Error::Io)?;

        if data_id != *b"data" {
            return Err(Error::wave(format!(
                "Malformed WAVE file: Expected \"data\" got {:?}",
                data_id,
            )));
        }

        let data_sz = u32::from_le_bytes(data_sz);

        let appendix_start = cursor
            .stream_position()
            .map_err(Error::Io)?
            .checked_add(data_sz.into())
            .unwrap();

        let new_wave_sz = appendix_start - wave_start - RIFF_HEAD_SZ;
        let new_wave_sz: u32 = new_wave_sz.try_into().map_err(Error::wave)?;

        Ok(Self {
            base_cursor: cursor,
            wave_start,
            wave_sz: new_wave_sz,
            appendix_start,
        })
    }

    pub fn appendix_cursor(&mut self) -> Result<&mut Cursor, io::Error> {
        self.base_cursor
            .seek(SeekFrom::Start(self.appendix_start))?;
        Ok(self.base_cursor)
    }

    pub fn wave_cursor(&mut self) -> Result<WaveCursor<Cursor>, io::Error> {
        self.base_cursor.seek(SeekFrom::Start(self.wave_start))?;
        WaveCursor::new(self.wave_sz, self.base_cursor)
    }
}

#[cfg(test)]
mod tests;
