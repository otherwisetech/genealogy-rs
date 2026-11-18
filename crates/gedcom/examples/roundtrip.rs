//! `cargo run -p gen-gedcom --example roundtrip -- FILE.ged [OUT.ged]`
//! Parses, prints stats, re-serializes, and checks the output parses to the same tree.
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: roundtrip FILE.ged [OUT.ged]");
    let t0 = std::time::Instant::now();
    let tree = gedcom::load(path.as_ref()).expect("read").expect("parse");
    let (p, f) = tree.stats();
    println!("{p} people, {f} families, parsed in {:?}", t0.elapsed());
    let out = tree.to_gedcom();
    let again = gedcom::Tree::parse(&out).expect("reparse");
    assert_eq!(again.records(), tree.records(), "round-trip changed the tree");
    println!("round-trip ok");
    if let Some(o) = args.next() {
        std::fs::write(o, out).expect("write");
    }
}
