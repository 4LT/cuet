use io::{Read, Seek, SeekFrom, Write};
use std::io;

pub const CHUNK_HEAD_SZ: usize = 8;
pub const CUE_SZ: usize = 24;

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
            Self::Wave(s) => writeln!(formatter, "Wave: {}", s)?,
            Self::Io(e) => writeln!(formatter, "IO: {}", e)?,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHead {
    pub tag: [u8; 4],
    pub size: u32,
}

impl ChunkHead {
    pub fn parse(cursor: &mut impl Read) -> Result<Self, Error> {
        let mut tag = [0u8; 4];
        let mut size_bytes = [0u8; 4];
        cursor.read_exact(&mut tag)?;
        cursor.read_exact(&mut size_bytes)?;
        let size = u32::from_le_bytes(size_bytes);

        Ok(ChunkHead { tag, size })
    }

    pub fn tag(&self) -> [u8; 4] {
        self.tag
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn as_bytes(&self) -> [u8; CHUNK_HEAD_SZ] {
        let mut bytes = [0u8; CHUNK_HEAD_SZ];
        bytes[..4].copy_from_slice(&self.tag[..]);
        bytes[4..].copy_from_slice(&self.size.to_le_bytes()[..]);
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CuePoint {
    pub id: u32,
    pub position: u32,
    pub data_tag: [u8; 4],
    pub chunk_start: u32,
    pub block_start: u32,
    pub sample_offset: u32,
}

impl CuePoint {
    fn parse(bytes: &[u8]) -> Self {
        let next_array = |iter: &mut std::slice::ChunksExact<'_, u8>| {
            *iter.next().unwrap().first_chunk::<4>().unwrap()
        };

        let next_int = |iter: &mut std::slice::ChunksExact<'_, u8>| {
            u32::from_le_bytes(next_array(iter))
        };

        let mut chunks = bytes.chunks_exact(4);
        let id = next_int(&mut chunks);
        let position = next_int(&mut chunks);
        let data_tag = next_array(&mut chunks);
        let chunk_start = next_int(&mut chunks);
        let block_start = next_int(&mut chunks);
        let sample_offset = next_int(&mut chunks);

        CuePoint {
            id,
            position,
            data_tag,
            chunk_start,
            block_start,
            sample_offset,
        }
    }

    pub fn from_sample_offset(id: u32, offset: u32) -> Self {
        CuePoint {
            id,
            position: 0,
            data_tag: *b"data",
            chunk_start: 0,
            block_start: 0,
            sample_offset: offset,
        }
    }

    pub fn as_bytes(&self) -> [u8; CUE_SZ] {
        let mut bytes = [0u8; CUE_SZ];
        bytes[..4].copy_from_slice(&self.id.to_le_bytes()[..]);
        bytes[4..8].copy_from_slice(&self.position.to_le_bytes()[..]);
        bytes[8..12].copy_from_slice(&self.data_tag[..]);
        bytes[12..16].copy_from_slice(&self.chunk_start.to_le_bytes()[..]);
        bytes[16..20].copy_from_slice(&self.block_start.to_le_bytes()[..]);
        bytes[20..].copy_from_slice(&self.sample_offset.to_le_bytes()[..]);
        bytes
    }
}

pub fn parse_cue_points(bytes: &[u8]) -> Vec<CuePoint> {
    (bytes[4..])
        .chunks_exact(CUE_SZ)
        .map(CuePoint::parse)
        .collect()
}

pub fn append_cue_chunk<Cursor: Read + Write + Seek>(
    cursor: &mut Cursor,
    cues: &[CuePoint],
) -> Result<(), Error> {
    let old_size = read_riff_head(cursor)?.size;
    let riff_sz_position = cursor.stream_position()? - 8;

    let chunk_size = cues
        .len()
        .checked_mul(CUE_SZ)
        .and_then(|sz| sz.checked_add(4))
        .and_then(|sz| u32::try_from(sz).ok())
        .ok_or(Error::wave("Chunk size exceeds bounds of 32-bit integer"))?;

    let new_size = chunk_size
        .checked_add(CHUNK_HEAD_SZ as u32)
        .and_then(|sz| sz.checked_add(old_size))
        .ok_or(Error::wave(
            "New RIFF size exceeds bounds of 32-bit integer",
        ))?;

    cursor.seek(SeekFrom::Start(riff_sz_position))?;
    cursor.write_all(&new_size.to_le_bytes()[..])?;
    cursor.seek(SeekFrom::Current(old_size.into()))?;

    let chunk_head = ChunkHead {
        tag: *b"cue ",
        size: chunk_size,
    };

    cursor.write_all(&chunk_head.as_bytes()[..])?;
    cursor.write_all(&(cues.len() as u32).to_le_bytes()[..])?;

    for cue in cues {
        cursor.write_all(&cue.as_bytes()[..])?;
    }

    Ok(())
}

fn read_riff_head<Cursor: Read + Seek>(
    cursor: &mut Cursor,
) -> Result<ChunkHead, Error> {
    let mut wave_id = [0u8; 4];
    let head = ChunkHead::parse(cursor)?;
    cursor.read_exact(&mut wave_id)?;

    if head.tag != *b"RIFF" || wave_id != *b"WAVE" {
        return Err(Error::wave("Not a WAVE file"));
    }

    if head.size & 1 == 1 {
        return Err(Error::wave("Malformed file: Odd RIFF size"));
    }

    Ok(head)
}

#[derive(Debug, PartialEq, Eq)]
pub struct WaveCursor<Cursor: Read + Seek> {
    head: ChunkHead,
    base_cursor: Cursor,
    wave_start: u64,
    wave_end: u64,
    first_chunk_pos: u64,
}

impl<Cursor: Read + Seek> WaveCursor<Cursor> {
    pub fn new(mut cursor: Cursor) -> Result<Self, Error> {
        let wave_start = cursor.stream_position()?;
        let head = read_riff_head(&mut cursor)?;
        let first_chunk_pos = cursor.stream_position()?;
        let wave_end = wave_start
            .checked_add(CHUNK_HEAD_SZ.try_into().unwrap())
            .and_then(|sz| sz.checked_add(head.size.into()))
            .ok_or(Error::wave("WAVE size too large for file"))?;

        Ok(Self {
            head,
            base_cursor: cursor,
            wave_start,
            wave_end,
            first_chunk_pos,
        })
    }

    pub fn reset(&mut self) -> Result<(), Error> {
        self.base_cursor
            .seek(SeekFrom::Start(self.first_chunk_pos))
            .map(|_| ())
            .map_err(Error::Io)
    }

    pub fn restore_cursor(mut self) -> Result<Cursor, Error> {
        self.base_cursor.seek(SeekFrom::Start(self.wave_start))?;
        Ok(self.base_cursor)
    }

    pub fn read_next_chunk_body(
        &mut self,
        tag: [u8; 4],
    ) -> Result<Option<Vec<u8>>, Error> {
        let current_position = |curs: &mut Cursor| curs.stream_position();

        let mut body = None;

        while current_position(&mut self.base_cursor)? < self.wave_end
            && body.is_none()
        {
            let chunk_head = ChunkHead::parse(&mut self.base_cursor)?;
            let size = chunk_head.size();

            if chunk_head.tag == tag {
                let mut buffer = vec![
                    0u8;
                    usize::try_from(size).map_err(|_| {
                        Error::wave(format!(
                            "Chunk size {} too large for platform",
                            size
                        ))
                    })?
                ];

                self.base_cursor.read_exact(&mut buffer[..])?;

                body = Some(buffer);
            } else {
                self.base_cursor.seek(SeekFrom::Current(size.into()))?;
            }

            if chunk_head.size & 1 == 1 {
                self.base_cursor.seek(SeekFrom::Current(1))?;
            }
        }

        Ok(body)
    }
}

#[cfg(test)]
mod tests;
