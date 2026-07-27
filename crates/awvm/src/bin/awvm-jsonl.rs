//! JSON Lines adapter over the AWVM reference implementation.
//!
//! The request handling lives in [`awvm::protocol`]; this binary is only the
//! stdin/stdout loop around it.

use std::io::{self, BufRead, Write};

use awvm::protocol::{Response, handle};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) if !line.trim().is_empty() => handle(&line),
            Ok(_) => continue,
            Err(e) => Response::error("", "IO_ERROR", e.to_string()),
        };
        serde_json::to_writer(&mut stdout, &response).expect("write response");
        writeln!(&mut stdout).expect("write newline");
        stdout.flush().expect("flush response");
    }
}
