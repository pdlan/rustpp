include!(concat!(env!("OUT_DIR"), "/generated.rs"));

fn main() {
    let point = Point::new(1.0, 2.0);
    assert_eq!(point.x, 1.0);
    assert_eq!(point.y, 3.0);
    println!("Rust++ value class: ({}, {})", point.x, point.y);
}
