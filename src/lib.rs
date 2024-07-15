use io::{Read, Seek, SeekFrom, Write};
use std::io;

pub const CHUNK_HEAD_SZ: usize = 8;
pub const CUE_SZ: usize = 24;
pub const LABELED_TEXT_MIN_SZ: usize = 20;
pub const CHUNK_TOO_BIG: &str = "Chunk size exceeds bounds of 32-bit integer";

pub type ChunkDefinition = ([u8; 4], Vec<u8>);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledText {
    pub cue_id: u32,
    pub sample_length: u32,
    pub purpose_id: [u8; 4],
    pub country: [u8; 2],
    pub language: [u8; 2],
    pub dialect: [u8; 2],
    pub code_page: u16,
    pub text: String,
}

impl LabeledText {
    // bytes length must be >= LABELED_TEXT_MIN_SZ
    fn parse(bytes: &[u8]) -> Self {
        let next_u32 = |iter: &mut std::slice::Iter<'_, u8>| {
            let u32_bytes = [
                *iter.next().unwrap(),
                *iter.next().unwrap(),
                *iter.next().unwrap(),
                *iter.next().unwrap(),
            ];
            u32::from_le_bytes(u32_bytes)
        };

        let next_u16 = |iter: &mut std::slice::Iter<'_, u8>| {
            let u16_bytes = [*iter.next().unwrap(), *iter.next().unwrap()];
            u16::from_le_bytes(u16_bytes)
        };

        let mut iter = bytes.iter();

        let cue_id = next_u32(&mut iter);
        let sample_length = next_u32(&mut iter);

        let purpose_id = [
            *iter.next().unwrap(),
            *iter.next().unwrap(),
            *iter.next().unwrap(),
            *iter.next().unwrap(),
        ];

        let country = [*iter.next().unwrap(), *iter.next().unwrap()];

        let language = [*iter.next().unwrap(), *iter.next().unwrap()];

        let dialect = [*iter.next().unwrap(), *iter.next().unwrap()];

        let code_page = next_u16(&mut iter);

        let text =
            String::from_utf8_lossy(&iter.copied().collect::<Vec<u8>>()[..])
                .to_string();

        LabeledText {
            cue_id,
            sample_length,
            purpose_id,
            country,
            language,
            dialect,
            code_page,
            text,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut text_bytes = Vec::from(self.text.as_bytes());
        let mut bytes =
            Vec::<u8>::with_capacity(LABELED_TEXT_MIN_SZ + text_bytes.len());

        bytes.extend_from_slice(&self.cue_id.to_le_bytes());
        bytes.extend_from_slice(&self.sample_length.to_le_bytes());
        bytes.extend_from_slice(&self.purpose_id);
        bytes.extend_from_slice(&self.country);
        bytes.extend_from_slice(&self.language);
        bytes.extend_from_slice(&self.dialect);
        bytes.extend_from_slice(&self.code_page.to_le_bytes());
        bytes.append(&mut text_bytes);

        bytes
    }

    pub fn from_cue_length(cue_id: u32, sample_length: u32) -> LabeledText {
        LabeledText {
            cue_id,
            sample_length,
            purpose_id: *b"mark",
            country: *b"  ",
            language: *b"  ",
            dialect: *b"  ",
            code_page: 0,
            text: String::from(""),
        }
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
    // bytes length must be CUE_SZ long
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

pub fn extract_labeled_text_from_list(bytes: &[u8]) -> Vec<LabeledText> {
    let mut labeled_texts = vec![];

    if bytes.len() < 4 {
        return labeled_texts;
    }

    let mut slice = &bytes[4..];

    while slice.len() >= 8 {
        let mut sub_chunk_len = [0u8; 4];

        sub_chunk_len.copy_from_slice(&slice[4..8]);
        let sub_chunk_len = u32::from_le_bytes(sub_chunk_len) as usize;
        let sub_chunk_tag = slice[..4].chunks(4).next().unwrap();
        slice = &slice[8..];

        if &sub_chunk_tag == b"ltxt"
            && sub_chunk_len >= LABELED_TEXT_MIN_SZ
            && slice.len() >= sub_chunk_len
        {
            let sub_chunk = &slice[..sub_chunk_len];
            labeled_texts.push(LabeledText::parse(sub_chunk));
        }

        slice = &slice[sub_chunk_len.min(slice.len())..];

        if sub_chunk_len & 1 == 1 && !slice.is_empty() {
            slice = &slice[1..];
        }
    }

    labeled_texts
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
        .ok_or(Error::wave(CHUNK_TOO_BIG))?;

    let new_size = chunk_size
        .checked_add(CHUNK_HEAD_SZ as u32)
        .and_then(|sz| sz.checked_add(old_size))
        .ok_or(Error::wave(CHUNK_TOO_BIG))?;

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

pub fn append_label_chunk<Cursor: Read + Write + Seek>(
    cursor: &mut Cursor,
    labeled_texts: &[LabeledText],
) -> Result<(), Error> {
    let old_size = read_riff_head(cursor)?.size;
    let riff_sz_position = cursor.stream_position()? - 8;

    let chunk_size = labeled_texts
        .iter()
        .map(|ltxt| {
            pad_size_16(ltxt.text.len())
                .and_then(|sz| sz.checked_add(LABELED_TEXT_MIN_SZ))
        })
        .try_fold(0usize, |accum, element| {
            element
                .and_then(|sz| sz.checked_add(accum))
                .and_then(|sum| sum.checked_add(CHUNK_HEAD_SZ))
        })
        .and_then(|sz| sz.checked_add(4usize))
        .and_then(|sz| u32::try_from(sz).ok())
        .ok_or(Error::wave(CHUNK_TOO_BIG))?;

    let new_size = chunk_size
        .checked_add(CHUNK_HEAD_SZ as u32)
        .and_then(|sz| sz.checked_add(old_size))
        .ok_or(Error::wave(CHUNK_TOO_BIG))?;

    cursor.seek(SeekFrom::Start(riff_sz_position))?;
    cursor.write_all(&new_size.to_le_bytes()[..])?;
    cursor.seek(SeekFrom::Current(old_size.into()))?;

    let chunk_head = ChunkHead {
        tag: *b"LIST",
        size: chunk_size,
    };

    cursor.write_all(&chunk_head.as_bytes()[..])?;
    cursor.write_all(b"adtl")?;

    for ltxt in labeled_texts {
        let sub_chunk_sz =
            u32::try_from(ltxt.text.len() + LABELED_TEXT_MIN_SZ).unwrap();

        let sub_chunk_head = ChunkHead {
            tag: *b"ltxt",
            size: sub_chunk_sz,
        };

        cursor.write_all(&sub_chunk_head.as_bytes()[..])?;
        cursor.write_all(&ltxt.as_bytes()[..])?;

        if sub_chunk_sz & 1 == 1 {
            cursor.write_all(&[0])?;
        }
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

fn pad_size_16(size: usize) -> Option<usize> {
    if size & 1 == 1 {
        size.checked_add(1)
    } else {
        Some(size)
    }
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

    pub fn read_next_chunk(
        &mut self,
        tag: Option<[u8; 4]>,
    ) -> Result<Option<ChunkDefinition>, Error> {
        let current_position = |curs: &mut Cursor| curs.stream_position();

        let mut chunk = None;

        while current_position(&mut self.base_cursor)? < self.wave_end
            && chunk.is_none()
        {
            let chunk_head = ChunkHead::parse(&mut self.base_cursor)?;
            let size = chunk_head.size();

            if tag.is_none() || Some(chunk_head.tag) == tag {
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

                chunk = Some((chunk_head.tag, buffer));
            } else {
                self.base_cursor.seek(SeekFrom::Current(size.into()))?;
            }

            if chunk_head.size & 1 == 1 {
                self.base_cursor.seek(SeekFrom::Current(1))?;
            }
        }

        Ok(chunk)
    }
}

#[cfg(test)]
mod tests;
