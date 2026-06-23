// Dev helper: rasterize an SVG to a PNG on a white background so icons can be
// eyeballed during design.
//
//   cargo run --example render_icon -- ui/icons/crowbar.svg out.png 256

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).expect("usage: render_icon <in.svg> <out.png> [size]");
    let output = args.get(2).expect("usage: render_icon <in.svg> <out.png> [size]");
    let size: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(256);

    use resvg::{tiny_skia, usvg};

    let data = std::fs::read(input).expect("read svg");
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(&data, &opt).expect("parse svg");

    let sx = size as f32 / tree.size().width();
    let sy = size as f32 / tree.size().height();
    let ts = tiny_skia::Transform::from_scale(sx, sy);

    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("alloc pixmap");
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, ts, &mut pixmap.as_mut());
    pixmap.save_png(output).expect("save png");
    println!("wrote {output} ({size}x{size})");
}
