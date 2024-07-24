use crate::{
    extract_labeled_text_from_list,
    parse_cue_points, ChunkHead, CuePoint, LabeledText, ChunkReader,
    ChunkWriter, CHUNK_HEAD_SZ, CUE_SZ,
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
            Some(bytes) => v.extend_from_slice(bytes),
        }

        riff_sz += rsz + CHUNK_HEAD_SZ as u32;
    }

    let riff_head = riff_head(riff_sz);
    v[..8].copy_from_slice(&riff_head.as_bytes()[..]);

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
        let bytes = wave_bytes(chunks);
        let mut base_cursor = io::Cursor::new(&bytes[..]);
        let initial_position = base_cursor.stream_position().unwrap();
        let mut cursor = ChunkReader::new(base_cursor).unwrap();

        let (_, chunk_bytes) =
            cursor.read_next_chunk(Some(*b"cue ")).unwrap().unwrap();

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
    let cues = [cue1, cue2];

    let fmt_head = ChunkHead {
        tag: *b"fmt ",
        size: 33,
    };

    let data_head = ChunkHead {
        tag: *b"data",
        size: 1,
    };

    let initial_wave_bytes = wave_bytes(&[(fmt_head, None), (data_head, None)]);
    let mut wave_bytes = initial_wave_bytes.clone();

    let mut cursor = io::Cursor::new(&mut wave_bytes);
    assert_eq!(cursor.stream_position().unwrap(), 0);
    let mut writer = ChunkWriter::new(cursor).unwrap();
    writer.append_cue_chunk(&cues[..]).unwrap();
    cursor = writer.restore_cursor().unwrap();
    assert_eq!(cursor.stream_position().unwrap(), 0);

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

    assert_eq!(
        wave_bytes.len(),
        initial_wave_bytes.len() + cue_bytes.len() + CHUNK_HEAD_SZ,
    );
}

#[test]
fn get_labeled_text() {
    let cue = CuePoint::from_sample_offset(1, 333);
    let mut ltxt1 = LabeledText::from_cue_length(1, 269);
    let ltxt2 = LabeledText::from_cue_length(13, 1234);
    ltxt1.text = String::from("hello");
    let ltxt1_bytes = ltxt1.as_bytes();
    let ltxt2_bytes = ltxt2.as_bytes();
    let mut cue_bytes = vec![1u8, 0, 0, 0];
    cue_bytes.extend_from_slice(&cue.as_bytes()[..]);
    let mut list_bytes = Vec::new();
    list_bytes.extend_from_slice(b"adtl");
    list_bytes.extend_from_slice(b"ltxt");
    list_bytes.extend_from_slice(&(ltxt1_bytes.len() as u32).to_le_bytes());
    list_bytes.extend_from_slice(&ltxt1_bytes);
    list_bytes.extend_from_slice(b"\0");
    list_bytes.extend_from_slice(b"ltxt");
    list_bytes.extend_from_slice(&(ltxt2_bytes.len() as u32).to_le_bytes());
    list_bytes.extend_from_slice(&ltxt2_bytes);

    let cue_head = ChunkHead {
        tag: *b"cue ",
        size: cue_bytes.len() as u32,
    };

    let list_head = ChunkHead {
        tag: *b"LIST",
        size: (ltxt1_bytes.len() + ltxt2_bytes.len()) as u32 + 21,
    };

    let chunks = [
        (cue_head, Some(&cue_bytes[..])),
        (list_head, Some(&list_bytes[..])),
    ];

    let bytes = wave_bytes(&chunks[..]);
    let base_cursor = io::Cursor::new(&bytes[..]);
    let mut cursor = ChunkReader::new(base_cursor).unwrap();

    let (_, chunk_bytes) =
        cursor.read_next_chunk(Some(*b"LIST")).unwrap().unwrap();

    let labeled_texts = extract_labeled_text_from_list(&chunk_bytes);

    assert_eq!(labeled_texts.len(), 2);
    assert_eq!(labeled_texts[0], ltxt1);
    assert_eq!(labeled_texts[1], ltxt2);
}

#[test]
fn append_labeled_text() {
    let cue = CuePoint::from_sample_offset(1, 123);
    let mut ltxt1 = LabeledText::from_cue_length(1, 456);
    let ltxt2 = LabeledText::from_cue_length(2, 2999);
    ltxt1.text = String::from("hello");
    let cues = [cue];
    let ltxt1_bytes = ltxt1.as_bytes();
    let ltxt2_bytes = ltxt2.as_bytes();
    let labeled_texts = [ltxt1, ltxt2];

    let mut cue_bytes = vec![1u8, 0, 0, 0];
    cue_bytes.extend_from_slice(&cue.as_bytes()[..]);

    let mut list_chunk_bytes = Vec::new();
    list_chunk_bytes.extend_from_slice(b"adtl");
    list_chunk_bytes.extend_from_slice(b"ltxt");
    list_chunk_bytes
        .extend_from_slice(&(ltxt1_bytes.len() as u32).to_le_bytes());
    list_chunk_bytes.extend_from_slice(&ltxt1_bytes);
    list_chunk_bytes.extend_from_slice(b"\0");
    list_chunk_bytes.extend_from_slice(b"ltxt");
    list_chunk_bytes
        .extend_from_slice(&(ltxt2_bytes.len() as u32).to_le_bytes());
    list_chunk_bytes.extend_from_slice(&ltxt2_bytes);

    let fmt_head = ChunkHead {
        tag: *b"fmt ",
        size: 33,
    };

    let data_head = ChunkHead {
        tag: *b"data",
        size: 1,
    };

    let initial_wave_bytes = wave_bytes(&[(fmt_head, None), (data_head, None)]);
    let mut wave_bytes = initial_wave_bytes.clone();

    let mut cursor = io::Cursor::new(&mut wave_bytes);
    assert_eq!(cursor.stream_position().unwrap(), 0);
    let mut writer = ChunkWriter::new(cursor).unwrap();
    writer.append_cue_chunk(&cues[..]).unwrap();
    writer.append_label_chunk(&labeled_texts[..]).unwrap();
    cursor = writer.restore_cursor().unwrap();
    assert_eq!(cursor.stream_position().unwrap(), 0);

    let cue_start = initial_wave_bytes.len();

    let list_start = cue_start
        + CHUNK_HEAD_SZ
        + u32::from_le_bytes(
            *wave_bytes[cue_start + 4..cue_start + 8]
                .first_chunk::<4>()
                .unwrap(),
        ) as usize;

    assert_eq!(&wave_bytes[list_start..list_start + 4], b"LIST");

    assert_eq!(
        &wave_bytes[list_start + 4..list_start + CHUNK_HEAD_SZ],
        &(list_chunk_bytes.len() as u32).to_le_bytes()
    );

    assert_eq!(&wave_bytes[list_start + CHUNK_HEAD_SZ..], &list_chunk_bytes,);
}
