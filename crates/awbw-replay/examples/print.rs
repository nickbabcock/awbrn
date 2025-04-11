use std::io::Read;

fn main() {
    let mut stdin = std::io::stdin();
    let mut buffer = Vec::new();
    stdin
        .read_to_end(&mut buffer)
        .expect("Failed to read from stdin");
    let replay = awbw_replay::parse_replay(&buffer).expect("Failed to parse replay");

    let stdout = std::io::stdout();
    let handle = stdout.lock();
    serde_json::to_writer_pretty(handle, &replay).expect("Failed to write to stdout");
}
