use std::io::Read;

use awbw_replay::ReplayParser;

fn main() {
    let mut stdin = std::io::stdin();
    let mut buffer = Vec::new();
    stdin.read_to_end(&mut buffer).expect("to read from stdin");
    let parser = ReplayParser::new().with_debug(true);
    let replay = parser.parse(&buffer).expect("to parse replay");

    let stdout = std::io::stdout();
    let handle = stdout.lock();
    serde_json::to_writer_pretty(handle, &replay).expect("to write to stdout");
}
