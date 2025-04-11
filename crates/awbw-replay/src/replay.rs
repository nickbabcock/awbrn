use crate::{
    de::deserialize_vec_pair,
    errors,
    game_models::AwbwGame,
    turn_models::{Action, TurnElement},
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read};

#[derive(Debug, Serialize, PartialEq, Clone)]
pub struct AwbwReplay {
    pub games: Vec<AwbwGame>,
    pub turns: Vec<Vec<Action>>,
}

enum FileType {
    Game,
    Turn,
}

pub fn parse_replay(data: &[u8]) -> Result<AwbwReplay, errors::ReplayError> {
    let zip = rawzip::ZipArchive::from_slice(data)?;
    let mut files = Vec::new();
    let mut entries = zip.entries();
    while let Some(entry) = entries.next_entry()? {
        if entry.is_dir() {
            continue;
        }

        if entry.compression_method() != rawzip::CompressionMethod::Deflate {
            continue;
        }

        files.push((entry.wayfinder(), entry.file_safe_path()?.into_owned()));
    }

    let mut games = Vec::new();
    let mut turns = Vec::new();
    let mut buf = Vec::new();

    for (wayfinder, _) in files {
        let entry = zip.get_entry(wayfinder)?;
        let reader = flate2::bufread::DeflateDecoder::new(entry.data());
        let reader = entry.verifying_reader(reader);
        let mut reader = std::io::BufReader::new(reader);

        let mut file_type = FileType::Game;
        let mut file = 0;
        loop {
            let is_eof = reader.fill_buf().map(|buf| buf.is_empty())?;
            if is_eof {
                break;
            }

            let mut reader = flate2::bufread::GzDecoder::new(&mut reader);
            buf.clear();

            reader.read_to_end(&mut buf)?;

            if file == 0 {
                file_type = if buf.starts_with(b"p:") {
                    FileType::Turn
                } else {
                    FileType::Game
                }
            }

            match file_type {
                FileType::Game => {
                    let mut deser = phpserz::PhpDeserializer::new(&buf);
                    let game: AwbwGame = AwbwGame::deserialize(&mut deser)?;
                    games.push(game);
                }
                FileType::Turn => {
                    let header = TurnHeader::from_slice(&buf).unwrap();
                    let mut deser = phpserz::PhpDeserializer::new(header.data);
                    let data: Vec<(u32, TurnElement)> = deserialize_vec_pair(&mut deser)?;
                    let actions: Vec<Action> = data
                        .into_iter()
                        .find_map(|(_, element)| match element {
                            TurnElement::Data(x) => Some(x),
                            _ => None,
                        })
                        .into_iter()
                        .flatten()
                        .collect();
                    turns.push(actions);
                }
            }

            file += 1;
        }
    }

    Ok(AwbwReplay { games, turns })
}

#[allow(dead_code)]
struct TurnHeader<'a> {
    player_id: u32,
    day: u32,
    data: &'a [u8],
}

impl<'a> TurnHeader<'a> {
    fn from_slice(data: &'a [u8]) -> Option<Self> {
        let (player_kind, data) = data.split_first_chunk::<2>()?;
        if player_kind != b"p:" {
            return None;
        }

        let player_id = data.iter().position(|&b| b == b'd')?;
        let (player_id, data) = data.split_at(player_id);
        let player_id = std::str::from_utf8(&player_id[..player_id.len() - 1]).ok()?;
        let player_id = player_id.parse::<u32>().ok()?;

        let (day_kind, data) = data.split_first_chunk::<2>()?;
        if day_kind != b"d:" {
            return None;
        }

        let day = data.iter().position(|&b| b == b'a')?;
        let (day, data) = data.split_at(day);
        let day = std::str::from_utf8(&day[..day.len() - 1]).ok()?;
        let day = day.parse::<u32>().ok()?;

        let (array_kind, data) = data.split_first_chunk::<2>()?;
        if array_kind != b"a:" {
            return None;
        }

        Some(TurnHeader {
            player_id,
            day,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_header() {
        let data = b"p:3189812;d:11;a:HELLO_WORLD";
        let header = TurnHeader::from_slice(data).unwrap();
        assert_eq!(header.player_id, 3189812);
        assert_eq!(header.day, 11);
        assert_eq!(header.data, b"HELLO_WORLD");
    }
}
