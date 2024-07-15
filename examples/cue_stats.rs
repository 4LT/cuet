use cuet::{extract_labeled_text_from_list, parse_cue_points, WaveCursor};
use std::env::args;
use std::fs::File;
use std::io;

fn main() {
    let mut arguments = args();

    let wav_path = if let Some(path) = arguments.nth(1) {
        path
    } else {
        panic!("No argument for path");
    };

    let file = File::open(wav_path).unwrap();
    let reader = io::BufReader::new(file);
    let mut wave_cursor = WaveCursor::new(reader).unwrap();

    let sample_byte_ct = wave_cursor
        .read_next_chunk(Some(*b"data"))
        .unwrap()
        .unwrap()
        .1
        .len();

    println!("Found {sample_byte_ct} bytes of samples");

    wave_cursor.reset().unwrap();

    let cue_body = wave_cursor.read_next_chunk(Some(*b"cue ")).unwrap();
    let list_body = wave_cursor.read_next_chunk(Some(*b"LIST")).unwrap();

    if let Some((_, payload)) = cue_body {
        let cue_points = parse_cue_points(&payload[..]);
        println!("{} cue points found", cue_points.len());

        for cue in cue_points {
            println!(
                "\t\"{}\" cue {} at sample {}",
                String::from_iter(cue.data_tag.iter().map(|ch| *ch as char)),
                cue.id,
                cue.sample_offset
            );
        }
    } else {
        println!("Cue chunk NOT found");
    }

    let list = list_body.and_then(|(_, payload)| {
        let ltxts = extract_labeled_text_from_list(&payload);

        if ltxts.is_empty() {
            None
        } else {
            Some(ltxts)
        }
    });

    if let Some(ltxts) = list {
        println!("{} labeled text cue desc. sub-chunks found", ltxts.len());
        for ltxt in ltxts {
            println!("\tcue {} has length {}", ltxt.cue_id, ltxt.sample_length);
        }
    } else {
        println!("Labeled text sub-chunk NOT found");
    }
}
