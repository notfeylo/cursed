//! What the import pipeline does to an image, dumped as pictures.
//!
//!     cargo run --bin matteprobe -- <image>
//!
//! Prints what the matte decided and writes the master it produced, so a
//! "the cursor looks broken" report can be answered by looking at the stage
//! that broke it rather than by guessing which one did.
use cursorforge_lib::build::{matte, pipeline};

fn main() {
    let path = std::env::args().nth(1).expect("usage: matteprobe <image>");
    let bytes = std::fs::read(&path).expect("read");
    let source = pipeline::decode(bytes).expect("decode").first().expect("a frame").clone();
    let out = std::env::temp_dir().join("cursorprobe");
    let _ = std::fs::create_dir_all(&out);

    let (w, h) = (source.width, source.height);
    let total = (w * h) as f32;
    let mut clear = 0u32;
    let mut opaque = 0u32;
    for y in 0..h {
        for x in 0..w {
            match source.alpha(x, y) {
                0 => clear += 1,
                255 => opaque += 1,
                _ => {}
            }
        }
    }
    println!("source {w}x{h}");
    println!(
        "  alpha: {:.1}% clear, {:.1}% opaque, {:.1}% soft",
        clear as f32 / total * 100.0,
        opaque as f32 / total * 100.0,
        (total - clear as f32 - opaque as f32) / total * 100.0
    );
    println!("  already_cut_out = {}", matte::already_cut_out(&source));
    println!("  assess          = {:?}", matte::assess(&source));

    match pipeline::prepare_master_reported(&source, pipeline::Cut::Auto) {
        Ok((master, report)) => {
            println!("  matte report    = {report:?}");
            println!("  master          = {}x{}", master.width, master.height);
            let mut m_opaque = 0u32;
            for y in 0..master.height {
                for x in 0..master.width {
                    if master.alpha(x, y) == 255 {
                        m_opaque += 1;
                    }
                }
            }
            println!(
                "  master opaque   = {:.1}%",
                m_opaque as f32 / (master.width * master.height) as f32 * 100.0
            );
            let png = out.join("master.png");
            std::fs::write(&png, master.to_png(image::codecs::png::CompressionType::Fast).unwrap())
                .unwrap();
            println!("  wrote {}", png.display());
        }
        Err(e) => println!("  prepare_master failed: {e}"),
    }
}
