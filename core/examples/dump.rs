use std::path::Path;
use volta_core::doc::Document;
use volta_core::epub::EpubDoc;

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump <epub>");
    let doc = EpubDoc::open(Path::new(&path)).expect("open failed");
    let ch: u32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let text = doc.chapter_text(ch);
    let paras: Vec<&str> = text.split("\n\n").collect();
    println!("chapter {} paragraphs: {}", ch, paras.len());
    for (i, p) in paras.iter().take(4).enumerate() {
        let preview: String = p.chars().take(80).collect();
        println!("  [{}] {}", i, preview);
    }
}
