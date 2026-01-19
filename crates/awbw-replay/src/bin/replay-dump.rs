use std::io::Read;

use awbw_replay::{ReplayEntriesKind, ReplayFile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = std::io::stdin();
    let mut buffer = Vec::new();
    stdin.read_to_end(&mut buffer)?;

    let file = ReplayFile::open(&buffer)?;
    let mut buf = Vec::new();

    for (file_entry_index, file_entry) in file.iter().enumerate() {
        let reader = std::io::BufReader::new(file_entry.get_reader()?);

        match ReplayEntriesKind::classify(reader)? {
            ReplayEntriesKind::Game(mut entries) => {
                let mut game_index = 0;
                while let Some(entry) = entries.next_entry(&mut buf)? {
                    println!("file[{}] game[{}]:", file_entry_index, game_index);
                    print_data(entry.data());
                    println!();
                    game_index += 1;
                }
            }
            ReplayEntriesKind::Turn(mut entries) => {
                let mut turn_index = 0;
                while let Some(entry) = entries.next_entry(&mut buf)? {
                    // Try to parse turn header to get player_id and day
                    let turn = match entry.parse() {
                        Ok(turn) => turn,
                        Err(e) => {
                            println!(
                                "file[{}] turn[{}] failed to parse: {}",
                                file_entry_index, turn_index, e
                            );
                            continue;
                        }
                    };

                    let player_id = turn.player_id();
                    let day = turn.day();

                    // Now get actions from the parsed turn
                    if let Ok(actions) = turn.actions() {
                        for (action_index, action_data) in actions.enumerate() {
                            println!(
                                "file[{}] turn[{}] (player={}, day={}) action[{}]:",
                                file_entry_index, turn_index, player_id, day, action_index
                            );
                            print_data(action_data.data());
                            println!();
                        }
                    }

                    turn_index += 1;
                }
            }
        }
    }

    Ok(())
}

fn print_data(data: &[u8]) {
    // Try to print as UTF-8 string (works for PHP and JSON data)
    match std::str::from_utf8(data) {
        Ok(s) => println!("{}", s),
        Err(_) => {
            // If not valid UTF-8, print as hex dump
            println!("<binary data: {} bytes>", data.len());
            for (i, chunk) in data.chunks(16).enumerate() {
                print!("{:08x}  ", i * 16);
                for byte in chunk {
                    print!("{:02x} ", byte);
                }
                println!();
            }
        }
    }
}
