use crate::{SplitCursor, WaveCursor};
use io::{Read, Seek, SeekFrom};
use std::io;

const RIFF_HEAD_SZ: usize = 8;
const WAVE_FMT_SZ: usize = 22; // not repr. of real wav file

fn riff_head(sz: u32) -> [u8; RIFF_HEAD_SZ] {
    let mut head = [0u8; RIFF_HEAD_SZ];
    (&mut head[0..4]).copy_from_slice(b"RIFF");
    (&mut head[4..RIFF_HEAD_SZ]).copy_from_slice(&sz.to_le_bytes());
    head
}

fn wave_bytes(samples_sz: u32, appendix_sz: u32) -> Vec<u8> {
    let mut v = Vec::<u8>::new();
    let riff_sz: u32 = 4 // "WAVE"
        + (RIFF_HEAD_SZ as u32) // "fmt " + sz
        + (WAVE_FMT_SZ as u32)
        + (RIFF_HEAD_SZ as u32) // "data" + sz
        + samples_sz
        + appendix_sz;

    v.extend_from_slice(&riff_head(riff_sz));
    v.extend_from_slice(b"WAVE");
    v.extend_from_slice(b"fmt\0");
    v.extend_from_slice(&(WAVE_FMT_SZ as u32).to_le_bytes());
    v.resize(v.len() + WAVE_FMT_SZ, 0);
    v.extend_from_slice(b"data");
    v.extend_from_slice(&samples_sz.to_le_bytes());
    v.resize(v.len() + (samples_sz as usize), 0);
    v.resize(v.len() + (appendix_sz as usize), 0);
    v
}

#[test]
fn wave_cursor_read_seek() {
    const SAMPLES_SZ: u32 = 88200;
    const APPENDIX_SZ: u32 = 200;
    const NEW_RIFF_SZ: u32 = 4
        + SAMPLES_SZ
        + WAVE_FMT_SZ as u32
        + 2 * RIFF_HEAD_SZ as u32;
    let bytes = wave_bytes(SAMPLES_SZ, APPENDIX_SZ);
    let mut base_cursor = io::Cursor::new(&bytes[..]);
    let mut split = SplitCursor::new(&mut base_cursor).unwrap();
    let mut wave_cursor = split.wave_cursor().unwrap();
    let new_riff_sz_bytes = NEW_RIFF_SZ.to_le_bytes();

    assert_eq!(wave_cursor.stream_position().unwrap(), 0);

    let mut trunc_bytes = [0u8; NEW_RIFF_SZ as usize + RIFF_HEAD_SZ];
    wave_cursor.read_exact(&mut trunc_bytes).unwrap();

    assert_eq!(&trunc_bytes[..4], b"RIFF");
    assert_eq!(&trunc_bytes[4..8], &new_riff_sz_bytes);
    assert_eq!(&trunc_bytes[8..12], b"WAVE");

    let mut word = [0u8; 4];
    let pos = wave_cursor
        .seek(SeekFrom::Start(RIFF_HEAD_SZ as u64))
        .unwrap();
    wave_cursor.read_exact(&mut word).unwrap();

    assert_eq!(pos, 8);
    assert_eq!(&word, b"WAVE");

    let pos = wave_cursor.seek(SeekFrom::Start(0)).unwrap();
    wave_cursor.read_exact(&mut word).unwrap();

    assert_eq!(pos, 0);
    assert_eq!(&word, b"RIFF");

    wave_cursor.read_exact(&mut word).unwrap();

    assert_eq!(&word, &new_riff_sz_bytes);

    wave_cursor.read_exact(&mut word).unwrap();

    assert_eq!(&word, b"WAVE");

    let mut dword = [0u8; 8];
    let pos = wave_cursor.seek(SeekFrom::Current(-10)).unwrap();
    wave_cursor.read_exact(&mut dword).unwrap();
    let mut expected_dword = [0u8; 8];
    expected_dword[0..2].copy_from_slice(b"FF");
    expected_dword[2..6].copy_from_slice(&new_riff_sz_bytes);
    expected_dword[6..8].copy_from_slice(b"WA");

    assert_eq!(pos, 2);
    assert_eq!(&dword, &expected_dword);
    assert_eq!(wave_cursor.stream_position().unwrap(), 10);

    let pos = wave_cursor.seek(SeekFrom::End(0)).unwrap();

    assert_eq!(pos, (RIFF_HEAD_SZ as u64) + (NEW_RIFF_SZ as u64));
}
