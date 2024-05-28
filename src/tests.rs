use crate::{
    append_cue_chunk, parse_cue_points, ChunkHead, CuePoint, WaveCursor,
    CHUNK_HEAD_SZ, CUE_SZ,
};
use io::Seek;
use std::io;

fn riff_head(size: u32) -> ChunkHead {
    ChunkHead {
        tag: *b"RIFF",
        size,
    }
}

fn real_size(size: u32) -> u32 {
    if size & 1 == 1 {
        size + 1
    } else {
        size
    }
}

fn wave_bytes(chunk_heads: &[(ChunkHead, Option<&[u8]>)]) -> Vec<u8> {
    let mut v = vec![0u8; CHUNK_HEAD_SZ];
    v.extend_from_slice(b"WAVE");
    let mut riff_sz = 4u32;

    for (head, payload) in chunk_heads {
        v.extend_from_slice(&head.as_bytes()[..]);
        let rsz = real_size(head.size);

        match payload {
            None => v.resize(v.len() + rsz as usize, 0u8),
            Some(bytes) => v.extend_from_slice(*bytes),
        }

        riff_sz += rsz + CHUNK_HEAD_SZ as u32;
    }

    let riff_head = riff_head(riff_sz);
    (&mut v[..8]).copy_from_slice(&riff_head.as_bytes()[..]);

    v
}

#[test]
fn get_cue_points() {
    let cue1 = CuePoint::from_sample_offset(1, 20);
    let cue2 = CuePoint::from_sample_offset(2, 200);
    let mut cue_bytes = vec![2u8, 0, 0, 0];
    cue_bytes.extend_from_slice(&cue1.as_bytes()[..]);
    cue_bytes.extend_from_slice(&cue2.as_bytes()[..]);
    let cue_head = ChunkHead {
        tag: *b"cue ",
        size: 4 + 2 * CUE_SZ as u32,
    };
    let fmt_head = ChunkHead {
        tag: *b"fmt ",
        size: 23,
    };
    let data_head = ChunkHead {
        tag: *b"data",
        size: 3001,
    };

    let check_chunks = |chunks: &[(ChunkHead, Option<&[u8]>)]| {
        let bytes = wave_bytes(&chunks[..]);
        let mut base_cursor = io::Cursor::new(&bytes[..]);
        let initial_position = base_cursor.stream_position().unwrap();
        let mut cursor = WaveCursor::new(base_cursor).unwrap();

        let chunk_bytes =
            cursor.read_next_chunk_body(*b"cue ").unwrap().unwrap();

        let cue_points = parse_cue_points(&chunk_bytes[..]);
        assert_eq!(cue_points[0], cue1);
        assert_eq!(cue_points[1], cue2);
        assert_eq!(cue_points.len(), 2);
        let mut base_cursor = cursor.restore_cursor().unwrap();
        assert_eq!(base_cursor.stream_position().unwrap(), initial_position);
    };

    check_chunks(
        &vec![
            (fmt_head, None),
            (data_head, None),
            (cue_head, Some(&cue_bytes[..])),
        ][..],
    );

    check_chunks(
        &vec![
            (fmt_head, None),
            (cue_head, Some(&cue_bytes[..])),
            (data_head, None),
        ][..],
    );

    check_chunks(
        &vec![
            (cue_head, Some(&cue_bytes[..])),
            (fmt_head, None),
            (data_head, None),
        ][..],
    );
}

#[test]
fn append_cue_points() {
    let cue1 = CuePoint::from_sample_offset(1, 333);
    let cue2 = CuePoint::from_sample_offset(2, 477);
    let mut cue_bytes = vec![2u8, 0, 0, 0];
    cue_bytes.extend_from_slice(&cue1.as_bytes()[..]);
    cue_bytes.extend_from_slice(&cue2.as_bytes()[..]);
    let cues = vec![cue1, cue2];

    let fmt_head = ChunkHead {
        tag: *b"fmt ",
        size: 33,
    };

    let data_head = ChunkHead {
        tag: *b"data",
        size: 1,
    };

    let initial_wave_bytes =
        wave_bytes(&vec![(fmt_head, None), (data_head, None)]);

    let mut wave_bytes = initial_wave_bytes.clone();

    let cursor_end_position = {
        let mut cursor = io::Cursor::new(&mut wave_bytes);
        assert_eq!(cursor.stream_position().unwrap(), 0);
        append_cue_chunk(&mut cursor, &cues[..]).unwrap();
        cursor.stream_position().unwrap()
    };

    assert_eq!(wave_bytes[..4], initial_wave_bytes[..4],);

    assert_eq!(
        wave_bytes[8..initial_wave_bytes.len()],
        initial_wave_bytes[8..],
    );

    assert_eq!(
        u32::from_le_bytes(*wave_bytes[4..8].first_chunk().unwrap()) as usize,
        u32::from_le_bytes(*initial_wave_bytes[4..8].first_chunk().unwrap())
            as usize
            + CHUNK_HEAD_SZ
            + cue_bytes.len(),
    );

    let cue_start = initial_wave_bytes.len();

    assert_eq!(
        wave_bytes[cue_start..cue_start + CHUNK_HEAD_SZ],
        ChunkHead {
            tag: *b"cue ",
            size: cue_bytes.len() as u32
        }
        .as_bytes()[..],
    );

    assert_eq!(wave_bytes[cue_start + CHUNK_HEAD_SZ..], cue_bytes[..]);

    assert_eq!(cursor_end_position, wave_bytes.len() as u64,);
    assert_eq!(
        wave_bytes.len(),
        initial_wave_bytes.len() + cue_bytes.len() + CHUNK_HEAD_SZ,
    );
}
