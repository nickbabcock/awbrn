//! JSON Lines adapter over the AWVM reference implementation.
//!
//! The request handling lives in [`awvm::protocol`]; this binary is only the
//! stdin/stdout loop around it.

use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) if !line.trim().is_empty() => awvm::protocol::handle(&line),
            Ok(_) => continue,
            Err(e) => serde_json::json!({
                "protocol_version": awvm::protocol::PROTOCOL_VERSION,
                "request_id": "",
                "status": "error",
                "code": "IO_ERROR",
                "message": e.to_string(),
            }),
        };
        serde_json::to_writer(&mut stdout, &response).expect("write response");
        writeln!(&mut stdout).expect("write newline");
        stdout.flush().expect("flush response");
    }
}
