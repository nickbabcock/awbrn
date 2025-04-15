use std::io::Read;

fn main() {
    let mut stdin = std::io::stdin();
    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .expect("Failed to read from stdin");
    let map = awbrn_map::AwbwMap::parse_txt(&buffer).expect("Failed to parse map");
    println!("{}", map);
}
