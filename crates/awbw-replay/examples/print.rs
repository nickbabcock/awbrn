use std::io::Read;

fn main() {
    let mut stdin = std::io::stdin();
    let mut buffer = Vec::new();
    stdin
        .read_to_end(&mut buffer)
        .expect("Failed to read from stdin");
    let (game, actions) = awbw_replay::parse_replay(&buffer).expect("Failed to parse replay");
    println!("Game: {}", game[0].id);
    for (i, action) in actions.iter().enumerate() {
        println!("Turn {i}: {action:?}");
    }
}
