use crate::{
    de::deserialize_vec_pair,
    errors::{self, ReplayError, ReplayErrorKind},
    game_models::AwbwGame,
    turn_models::{Action, TurnElement},
};
use phpserz::{PhpParser, PhpToken};
use rawzip::{ZipSliceArchive, path::ZipFilePath};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read};

#[derive(Debug, Serialize, PartialEq, Clone)]
pub struct AwbwReplay {
    pub games: Vec<AwbwGame>,
    pub turns: Vec<Action>,
}

#[derive(Debug)]
struct ZipFileEntry {
    wayfinder: rawzip::ZipArchiveEntryWayfinder,
    file_name_range: (usize, usize),
}

#[derive(Debug)]
pub struct ReplayFile<R> {
    zip: ZipSliceArchive<R>,
    file_entries: Vec<ZipFileEntry>,
    file_name_data: Vec<u8>,
}

impl<R: AsRef<[u8]>> ReplayFile<R> {
    pub fn open(data: R) -> Result<Self, errors::ReplayError> {
        let zip = rawzip::ZipArchive::from_slice(data)?;
        let mut files = Vec::new();
        let mut entries = zip.entries();
        let mut file_name_data = Vec::new();
        while let Some(entry) = entries.next_entry()? {
            if entry.is_dir() {
                continue;
            }

            if entry.compression_method() != rawzip::CompressionMethod::Deflate {
                continue;
            }

            let start = file_name_data.len();
            file_name_data.extend_from_slice(entry.file_path().as_bytes());
            let end = file_name_data.len();
            files.push(ZipFileEntry {
                wayfinder: entry.wayfinder(),
                file_name_range: (start, end),
            })
        }

        Ok(ReplayFile {
            zip,
            file_entries: files,
            file_name_data,
        })
    }

    pub fn iter(&self) -> ReplayFileIterator<'_, R> {
        ReplayFileIterator {
            file: self,
            index: 0,
        }
    }
}

#[derive(Debug)]
pub struct ReplayFileIterator<'a, R: AsRef<[u8]>> {
    file: &'a ReplayFile<R>,
    index: usize,
}

impl<'a, R: AsRef<[u8]>> Iterator for ReplayFileIterator<'a, R> {
    type Item = ReplayFileEntry<'a, R>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.file.file_entries.get(self.index)?;
        let file_name = &self.file.file_name_data[entry.file_name_range.0..entry.file_name_range.1];
        self.index += 1;
        Some(ReplayFileEntry {
            wayfinder: entry.wayfinder,
            file: self.file,
            file_name,
        })
    }
}

pub struct ReplayFileEntry<'a, R: AsRef<[u8]>> {
    wayfinder: rawzip::ZipArchiveEntryWayfinder,
    file_name: &'a [u8],
    file: &'a ReplayFile<R>,
}

impl<'a, R: AsRef<[u8]>> ReplayFileEntry<'a, R> {
    pub fn file_path(&self) -> ZipFilePath<rawzip::path::RawPath<'a>> {
        ZipFilePath::from_bytes(self.file_name)
    }

    pub fn uncompressed_size_hint(&self) -> u64 {
        self.wayfinder.uncompressed_size_hint()
    }

    pub fn get_reader(&self) -> Result<impl Read, errors::ReplayError> {
        let entry = self.file.zip.get_entry(self.wayfinder)?;
        let reader = flate2::bufread::DeflateDecoder::new(entry.data());
        Ok(entry.verifying_reader(reader))
    }
}

pub struct GameKind;
pub struct TurnKind;

pub enum ReplayEntriesKind<R> {
    Game(ReplayEntries<GameKind, R>),
    Turn(ReplayEntries<TurnKind, R>),
}

impl<R: BufRead> ReplayEntriesKind<R> {
    pub fn classify(mut reader: R) -> Result<Self, errors::ReplayError> {
        let buf = reader.fill_buf()?;
        let mut decoder = flate2::bufread::GzDecoder::new(buf);
        let mut peek_data = [0u8; 2];
        decoder.read_exact(&mut peek_data)?;
        if peek_data == *b"p:" {
            Ok(ReplayEntriesKind::Turn(ReplayEntries {
                reader,
                marker: std::marker::PhantomData,
            }))
        } else {
            Ok(ReplayEntriesKind::Game(ReplayEntries {
                reader,
                marker: std::marker::PhantomData,
            }))
        }
    }
}

#[derive(Debug)]
pub struct ReplayEntries<T, R> {
    reader: R,
    marker: std::marker::PhantomData<T>,
}

impl<T, R: BufRead> ReplayEntries<T, R> {
    pub fn next_entry<'a>(
        &mut self,
        sink: &'a mut Vec<u8>,
    ) -> Result<Option<ReplayEntry<'a, T>>, errors::ReplayError> {
        let is_eof = self.reader.fill_buf().map(|buf| buf.is_empty())?;
        if is_eof {
            return Ok(None);
        }

        let mut reader = flate2::bufread::GzDecoder::new(&mut self.reader);
        sink.clear();
        reader.read_to_end(sink)?;

        Ok(Some(ReplayEntry {
            data: sink,
            marker: std::marker::PhantomData,
        }))
    }
}

#[derive(Debug)]
pub struct ReplayEntry<'a, T> {
    data: &'a [u8],
    marker: std::marker::PhantomData<T>,
}

impl<'a, T> ReplayEntry<'a, T> {
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

impl<'a> ReplayEntry<'a, GameKind> {
    pub fn deserializer(&self) -> phpserz::PhpDeserializer<'a> {
        phpserz::PhpDeserializer::new(self.data())
    }
}

impl<'a> ReplayEntry<'a, TurnKind> {
    pub fn parse(&self) -> Result<TurnContent<'a>, errors::ReplayError> {
        TurnContent::from_slice(self.data).ok_or(ReplayError {
            kind: ReplayErrorKind::InvalidTurnData { context: None },
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct ReplayParser {
    debug: bool,
}

impl ReplayParser {
    pub fn new() -> Self {
        ReplayParser::default()
    }

    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn parse(&self, data: &[u8]) -> Result<AwbwReplay, errors::ReplayError> {
        let file = ReplayFile::open(data)?;

        let mut games = Vec::new();
        let mut turns = Vec::new();
        let mut buf = Vec::new();

        for (file_entry_index, file_entry) in file.iter().enumerate() {
            let reader = std::io::BufReader::new(file_entry.get_reader()?);

            match ReplayEntriesKind::classify(reader)? {
                ReplayEntriesKind::Game(mut entries) => {
                    let mut game_index = 0;
                    while let Some(entry) = entries.next_entry(&mut buf)? {
                        let mut deser = entry.deserializer();
                        let game = if self.debug {
                            let mut track = serde_path_to_error::Track::new();
                            let path_deser =
                                serde_path_to_error::Deserializer::new(&mut deser, &mut track);
                            AwbwGame::deserialize(path_deser).map_err(|error| ReplayError {
                                kind: ReplayErrorKind::Php {
                                    error,
                                    path: Some(track.path()),
                                    context: Some(errors::DeserializationContext {
                                        file_entry_index,
                                        entry_kind: errors::EntryKind::Game { game_index },
                                    }),
                                },
                            })?
                        } else {
                            AwbwGame::deserialize(&mut deser).map_err(|error| ReplayError {
                                kind: ReplayErrorKind::Php {
                                    error,
                                    path: None,
                                    context: Some(errors::DeserializationContext {
                                        file_entry_index,
                                        entry_kind: errors::EntryKind::Game { game_index },
                                    }),
                                },
                            })?
                        };

                        games.push(game);
                        game_index += 1;
                    }
                }
                ReplayEntriesKind::Turn(mut entries) => {
                    let mut turn_index = 0;
                    while let Some(entry) = entries.next_entry(&mut buf)? {
                        let turn = entry.parse().map_err(|_| ReplayError {
                            kind: ReplayErrorKind::InvalidTurnData {
                                context: Some(errors::DeserializationContext {
                                    file_entry_index,
                                    entry_kind: errors::EntryKind::Turn {
                                        turn_index,
                                        player_id: 0,
                                        day: 0,
                                        action_index: None,
                                    },
                                }),
                            },
                        })?;

                        let player_id = turn.player_id();
                        let day = turn.day();

                        for element in turn.actions()? {
                            let mut deser = element.deserializer();
                            let action = if self.debug {
                                let mut track = serde_path_to_error::Track::new();
                                let path_deser =
                                    serde_path_to_error::Deserializer::new(&mut deser, &mut track);
                                Action::deserialize(path_deser).map_err(|error| ReplayError {
                                    kind: ReplayErrorKind::Json {
                                        error,
                                        path: Some(track.path()),
                                        context: Some(errors::DeserializationContext {
                                            file_entry_index,
                                            entry_kind: errors::EntryKind::Turn {
                                                turn_index,
                                                player_id,
                                                day,
                                                action_index: Some(turns.len()),
                                            },
                                        }),
                                    },
                                })?
                            } else {
                                Action::deserialize(&mut deser).map_err(|error| ReplayError {
                                    kind: ReplayErrorKind::Json {
                                        error,
                                        path: None,
                                        context: Some(errors::DeserializationContext {
                                            file_entry_index,
                                            entry_kind: errors::EntryKind::Turn {
                                                turn_index,
                                                player_id,
                                                day,
                                                action_index: Some(turns.len()),
                                            },
                                        }),
                                    },
                                })?
                            };
                            turns.push(action);
                        }
                        turn_index += 1;
                    }
                }
            }
        }

        Ok(AwbwReplay { games, turns })
    }
}

pub struct TurnContent<'a> {
    player_id: u32,
    day: u32,
    data: &'a [u8],
}

impl<'a> TurnContent<'a> {
    fn from_slice(data: &'a [u8]) -> Option<Self> {
        let (player_kind, data) = data.split_first_chunk::<2>()?;
        if player_kind != b"p:" {
            return None;
        }

        let player_id = data.iter().position(|&b| b == b'd')?;
        let (player_id, data) = data.split_at(player_id);
        let player_id = std::str::from_utf8(&player_id[..player_id.len() - 1]).ok()?;
        let player_id = player_id.parse::<u32>().ok()?;

        let mut parser = PhpParser::new(data);

        let PhpToken::Float(day) = parser.read_token().ok()? else {
            return None;
        };

        let (array_kind, data) = data[parser.position()..].split_first_chunk::<2>()?;
        if array_kind != b"a:" {
            return None;
        }

        Some(TurnContent {
            player_id,
            day: day as u32,
            data,
        })
    }

    pub fn data(&self) -> &[u8] {
        self.data
    }

    pub fn player_id(&self) -> u32 {
        self.player_id
    }

    pub fn day(&self) -> u32 {
        self.day
    }

    pub fn actions(&'a self) -> Result<impl Iterator<Item = ActionData<'a>>, errors::ReplayError> {
        let mut deser = phpserz::PhpDeserializer::new(self.data());
        let data: Vec<(u32, TurnElement<'a>)> = deserialize_vec_pair(&mut deser)?;

        let result = data
            .into_iter()
            .find_map(|(_, element)| match element {
                TurnElement::Data(x) => Some(x),
                _ => None,
            })
            .into_iter()
            .flatten()
            .map(|data| ActionData { data });

        Ok(result)
    }
}

#[derive(Debug)]
pub struct ActionData<'a> {
    data: &'a [u8],
}

impl<'a> ActionData<'a> {
    pub fn data(&self) -> &[u8] {
        self.data
    }

    pub fn deserializer(&self) -> serde_json::Deserializer<serde_json::de::SliceRead<'a>> {
        serde_json::Deserializer::from_slice(self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_header() {
        let data = b"p:3189812;d:11;a:HELLO_WORLD";
        let header = TurnContent::from_slice(data).unwrap();
        assert_eq!(header.player_id, 3189812);
        assert_eq!(header.day, 11);
        assert_eq!(header.data, b"HELLO_WORLD");
    }
}
