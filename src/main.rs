mod fs;

fn main() {
    let entries = fs::read_dir(std::path::Path::new("."), false).unwrap();
    for e in entries {
        println!("{} {}", e.name, e.size_label());
    }
}
