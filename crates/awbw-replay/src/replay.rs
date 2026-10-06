use crate::{
    errors::{self, ReplayError, ReplayErrorKind},
    game_models::AwbwGame,
    turn_models::Action,
};
use phpserz::{PhpParser, PhpToken};
use rawzip::{ZipSliceArchive, ZipVerification, path::ZipFilePath};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read};

// Per ZIP entry, across all gzip members. Supported fixtures are below 24 MiB.
const MAX_DECOMPRESSED_BYTES: usize = 128 * 1024 * 1024;

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

            if entry.compression_method() != rawzip::CompressionMethod::DEFLATE {
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

#[derive(Debug)]
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

        // Use flate2 to verify the CRC as it delegates to CRC implementations
        // that can take advantage of hardware acceleration when available.
        Ok(VerifyingReader {
            reader: flate2::CrcReader::new(reader),
            verifier: entry.claim_verifier(),
        })
    }
}

struct VerifyingReader<'a> {
    reader: flate2::CrcReader<flate2::bufread::DeflateDecoder<&'a [u8]>>,
    verifier: rawzip::ZipVerification,
}

impl Read for VerifyingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(buf)?;
        if n == 0 {
            self.verifier
                .valid(ZipVerification {
                    crc: self.reader.crc().sum(),
                    uncompressed_size: self.reader.get_ref().total_out(),
                })
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        }
        Ok(n)
    }
}

#[derive(Debug)]
pub struct GameKind;
#[derive(Debug)]
pub struct TurnKind;

#[derive(Debug)]
pub enum ReplayEntriesKind<R> {
    Game(ReplayEntries<GameKind, R>),
    Turn(ReplayEntries<TurnKind, R>),
}

impl<R: BufRead> ReplayEntriesKind<R> {
    /// Use the record prefix to identify game or turn data.
    pub fn classify(mut reader: R) -> Result<Self, errors::ReplayError> {
        let buf = reader.fill_buf()?;
        let mut decoder = flate2::bufread::MultiGzDecoder::new(buf);
        let mut peek_data = [0u8; 2];
        decoder.read_exact(&mut peek_data)?;
        if peek_data == *b"p:" {
            Ok(ReplayEntriesKind::Turn(ReplayEntries {
                reader,
                data: Vec::new(),
                position: 0,
                marker: std::marker::PhantomData,
            }))
        } else {
            Ok(ReplayEntriesKind::Game(ReplayEntries {
                reader,
                data: Vec::new(),
                position: 0,
                marker: std::marker::PhantomData,
            }))
        }
    }
}

#[derive(Debug)]
pub struct ReplayEntries<T, R> {
    reader: R,
    data: Vec<u8>,
    position: usize,
    marker: std::marker::PhantomData<T>,
}

impl<T, R: BufRead> ReplayEntries<T, R> {
    /// Read the next PHP record from the gzip stream.
    pub fn next_entry<'a>(
        &mut self,
        sink: &'a mut Vec<u8>,
    ) -> Result<Option<ReplayEntry<'a, T>>, errors::ReplayError> {
        loop {
            while self
                .data
                .get(self.position)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.position += 1;
            }
            if self.position < self.data.len() {
                break;
            }
            if self.reader.fill_buf()?.is_empty() {
                return Ok(None);
            }

            let reader = flate2::bufread::MultiGzDecoder::new(&mut self.reader);
            self.data.clear();
            reader
                .take((MAX_DECOMPRESSED_BYTES + 1) as u64)
                .read_to_end(&mut self.data)?;
            if self.data.len() > MAX_DECOMPRESSED_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "decompressed replay exceeds the 128 MiB limit",
                )
                .into());
            }
            self.position = 0;
        }

        // PHP record boundaries can differ from gzip member boundaries.
        let data = &self.data[self.position..];
        let php_data = if data.starts_with(b"p:") {
            TurnContent::from_slice(data)
                .ok_or(ReplayError {
                    kind: ReplayErrorKind::InvalidTurnData { context: None },
                })?
                .data
        } else {
            data
        };
        let mut deser = phpserz::PhpDeserializer::new(php_data);
        serde::de::IgnoredAny::deserialize(&mut deser)?;
        let len = data.len() - php_data.len() + deser.into_parser().position();
        sink.clear();
        sink.extend_from_slice(&data[..len]);
        self.position += len;

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

#[derive(Debug)]
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
        let (_, _, action_data): (serde::de::IgnoredAny, serde::de::IgnoredAny, Vec<&'a [u8]>) =
            Deserialize::deserialize(&mut deser)?;

        Ok(action_data.into_iter().map(|data| ActionData { data }))
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
    use std::io::Write;

    fn gzip_members(members: &[&[u8]]) -> Vec<u8> {
        let mut data = Vec::new();
        for member in members {
            let mut encoder = flate2::write::GzEncoder::new(&mut data, flate2::Compression::fast());
            encoder.write_all(member).unwrap();
            encoder.finish().unwrap();
        }
        data
    }

    #[test]
    fn packed_gzip_members_yield_all_game_records() {
        let first = b"O:4:\"Game\":1:{s:3:\"day\";i:1;}";
        let second = b"O:4:\"Game\":1:{s:3:\"day\";i:2;}";
        let third = b"O:4:\"Game\":1:{s:3:\"day\";i:3;}";
        let packed = [first.as_slice(), b"\n", second.as_slice(), b"\n"].concat();
        let data = gzip_members(&[&packed, b"", third]);
        let ReplayEntriesKind::Game(mut entries) =
            ReplayEntriesKind::classify(data.as_slice()).unwrap()
        else {
            panic!("expected game records");
        };
        let mut sink = Vec::new();
        for expected in [first, second, third] {
            assert_eq!(
                entries.next_entry(&mut sink).unwrap().unwrap().data(),
                expected
            );
        }
        assert!(entries.next_entry(&mut sink).unwrap().is_none());
    }

    #[test]
    fn packed_gzip_members_yield_all_turn_records() {
        let first = b"p:1;d:1;a:a:3:{i:0;i:1;i:1;i:1;i:2;a:1:{i:0;s:4:\"p:2;\";}}";
        let second = b"p:2;d:1;a:a:3:{i:0;i:2;i:1;i:1;i:2;a:1:{i:0;s:2:\"bb\";}}";
        let third = b"p:1;d:2;a:a:3:{i:0;i:1;i:1;i:2;i:2;a:1:{i:0;s:2:\"cc\";}}";
        let packed = [first.as_slice(), b"\n", second.as_slice(), b"\n"].concat();
        let data = gzip_members(&[&packed, third]);
        let ReplayEntriesKind::Turn(mut entries) =
            ReplayEntriesKind::classify(data.as_slice()).unwrap()
        else {
            panic!("expected turn records");
        };
        let mut sink = Vec::new();
        for (player, day, action) in [(1, 1, b"p:2;".as_slice()), (2, 1, b"bb"), (1, 2, b"cc")] {
            let entry = entries.next_entry(&mut sink).unwrap().unwrap();
            let turn = entry.parse().unwrap();
            assert_eq!((turn.player_id(), turn.day()), (player, day));
            let actions = turn
                .actions()
                .unwrap()
                .map(|action| action.data)
                .collect::<Vec<_>>();
            assert_eq!(actions, [action]);
        }
        assert!(entries.next_entry(&mut sink).unwrap().is_none());
    }

    #[test]
    fn gzip_members_can_split_game_records() {
        let first = b"O:4:\"Game\":1:{s:3:\"day\";i:1;}";
        let second = b"O:4:\"Game\":1:{s:3:\"day\";i:2;}";
        let packed = [first.as_slice(), b"\n", second.as_slice()].concat();
        for split in 0..=packed.len() {
            let data = gzip_members(&[&packed[..split], &packed[split..]]);
            let ReplayEntriesKind::Game(mut entries) =
                ReplayEntriesKind::classify(data.as_slice()).unwrap()
            else {
                panic!("expected game records");
            };
            let mut sink = Vec::new();
            for expected in [first.as_slice(), second.as_slice()] {
                assert_eq!(
                    entries.next_entry(&mut sink).unwrap().unwrap().data(),
                    expected,
                    "gzip member boundary at {split}"
                );
            }
            assert!(entries.next_entry(&mut sink).unwrap().is_none());
        }
    }

    #[test]
    fn gzip_members_can_split_turn_records() {
        let first = b"p:1;d:1;a:a:3:{i:0;i:1;i:1;i:1;i:2;a:1:{i:0;s:4:\"p:2;\";}}";
        let second = b"p:2;d:1;a:a:3:{i:0;i:2;i:1;i:1;i:2;a:1:{i:0;s:2:\"bb\";}}";
        let packed = [first.as_slice(), b"\n", second.as_slice()].concat();
        for split in 0..=packed.len() {
            let data = gzip_members(&[&packed[..split], &packed[split..]]);
            let ReplayEntriesKind::Turn(mut entries) =
                ReplayEntriesKind::classify(data.as_slice()).unwrap()
            else {
                panic!("expected turn records");
            };
            let mut sink = Vec::new();
            for expected in [first.as_slice(), second.as_slice()] {
                assert_eq!(
                    entries.next_entry(&mut sink).unwrap().unwrap().data(),
                    expected,
                    "gzip member boundary at {split}"
                );
            }
            assert!(entries.next_entry(&mut sink).unwrap().is_none());
        }
    }

    #[test]
    fn packed_gzip_members_reject_invalid_trailing_records() {
        let first = b"O:4:\"Game\":1:{s:3:\"day\";i:1;}";
        let packed = [first.as_slice(), b"\nO:4:\"Game\":1:{"].concat();
        let data = gzip_members(&[&packed]);
        let ReplayEntriesKind::Game(mut entries) =
            ReplayEntriesKind::classify(data.as_slice()).unwrap()
        else {
            panic!("expected game records");
        };
        let mut sink = Vec::new();
        assert_eq!(
            entries.next_entry(&mut sink).unwrap().unwrap().data(),
            first
        );
        entries.next_entry(&mut sink).unwrap_err();
    }

    fn padded_game_stream(size: usize) -> Vec<u8> {
        let record = b"O:4:\"Game\":1:{s:3:\"day\";i:1;}";
        let mut data = gzip_members(&[record]);
        // Put padding in another member to verify that the limit spans members.
        let mut encoder = flate2::write::GzEncoder::new(&mut data, flate2::Compression::fast());
        std::io::copy(
            &mut std::io::repeat(b' ').take((size - record.len()) as u64),
            &mut encoder,
        )
        .unwrap();
        encoder.finish().unwrap();
        data
    }

    #[test]
    fn decompressed_streams_at_or_below_the_limit_are_complete() {
        for size in [MAX_DECOMPRESSED_BYTES - 1, MAX_DECOMPRESSED_BYTES] {
            let data = padded_game_stream(size);
            let ReplayEntriesKind::Game(mut entries) =
                ReplayEntriesKind::classify(data.as_slice()).unwrap()
            else {
                panic!("expected game records");
            };
            let mut sink = Vec::new();
            assert_eq!(
                entries.next_entry(&mut sink).unwrap().unwrap().data(),
                b"O:4:\"Game\":1:{s:3:\"day\";i:1;}"
            );
            assert_eq!(entries.data.len(), size);
            assert!(entries.next_entry(&mut sink).unwrap().is_none());
        }
    }

    #[test]
    fn oversized_decompressed_streams_fail_before_yielding_records() {
        let data = padded_game_stream(MAX_DECOMPRESSED_BYTES + 2);
        let ReplayEntriesKind::Game(mut entries) =
            ReplayEntriesKind::classify(data.as_slice()).unwrap()
        else {
            panic!("expected game records");
        };
        let error = entries.next_entry(&mut Vec::new()).unwrap_err();
        let ReplayErrorKind::Io(error) = error.kind else {
            panic!("expected an IO error");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(entries.data.len(), MAX_DECOMPRESSED_BYTES + 1);
    }

    #[test]
    fn test_turn_header() {
        let data = b"p:3189812;d:11;a:HELLO_WORLD";
        let header = TurnContent::from_slice(data).unwrap();
        assert_eq!(header.player_id, 3189812);
        assert_eq!(header.day, 11);
        assert_eq!(header.data, b"HELLO_WORLD");
    }

    #[test]
    fn test_turn_actions_with_multiple_entries() {
        let data =
            b"p:3189394;d:1;a:a:3:{i:0;i:3189394;i:1;i:1;i:2;a:3:{i:0;s:2:\"aa\";i:1;s:2:\"bb\";i:2;s:2:\"cc\";}}";
        let turn = TurnContent::from_slice(data).unwrap();
        let actions = turn
            .actions()
            .unwrap()
            .map(|action| action.data().to_vec())
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![b"aa".to_vec(), b"bb".to_vec(), b"cc".to_vec()]
        );
    }
}
