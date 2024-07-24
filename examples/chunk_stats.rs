use cuet::ChunkReader;
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
    let mut wave_cursor = ChunkReader::new(reader).unwrap();

    while let Some((tag, chunk)) = wave_cursor.read_next_chunk(None).unwrap() {
        let tag_s = tag.iter().map(|&b| b as char).collect::<String>();
        println!(
            "Found \"{}\" chunk that's {} bytes long",
            tag_s,
            chunk.len()
        );

        if tag == *b"LIST" {
            let ltype = &chunk[..4];
            let ltype_s = ltype.iter().map(|&b| b as char).collect::<String>();
            println!("\tLIST type = \"{}\"", ltype_s);

            if ltype == *b"adtl" {
                let sctype = &chunk[4..8];
                let sctype_s =
                    sctype.iter().map(|&b| b as char).collect::<String>();
                println!("\tSub-chunk type = \"{}\"", sctype_s);

                let sclen = u32::from_le_bytes(
                    *chunk[8..12]
                        .chunks_exact(4)
                        .next()
                        .unwrap()
                        .first_chunk::<4>()
                        .unwrap(),
                ) as usize;

                println!("\tSub-chunk length: {}", sclen);
            }
        }
    }
}
