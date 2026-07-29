//! Helpers shared by the integration suites.

use std::io::BufWriter;
use std::path::{Path, PathBuf};

use highway::HighwayHash;

/// Hash `bytes` with an explicit length frame so concatenated inputs cannot
/// collide with a differently split pair.
pub fn append_framed(hasher: &mut highway::HighwayHasher, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect("serialized length fits u64");
    hasher.append(&length.to_le_bytes());
    hasher.append(bytes);
}

/// Hash `value`'s JSON encoding without materializing it.
///
/// Serializing into the hasher keeps the same framing as [`append_framed`] --
/// the byte count is known once the value is written -- while avoiding the
/// buffer entirely. These suites hash gigabytes of observations, so that
/// buffer was pure allocation churn.
pub fn append_framed_json(hasher: &mut highway::HighwayHasher, value: &impl serde::Serialize) {
    // Buffered: the hasher is much faster over a few large blocks than over the
    // many small writes serde_json emits.
    let mut writer = BufWriter::with_capacity(0x8000, CountingHasher { hasher, written: 0 });
    serde_json::to_writer(&mut writer, value).expect("observations serialize as JSON");
    let Ok(counter) = writer.into_inner() else {
        unreachable!("hashing a serialized observation cannot fail");
    };
    let written = counter.written;
    let length = u64::try_from(written).expect("serialized length fits u64");
    hasher.append(&length.to_le_bytes());
}

struct CountingHasher<'a> {
    hasher: &'a mut highway::HighwayHasher,
    written: usize,
}

impl std::io::Write for CountingHasher<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hasher.append(buf);
        self.written += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn map_path(file_name: &str) -> PathBuf {
    workspace_path("assets/maps").join(file_name)
}

pub fn replay_path(file_name: &str) -> PathBuf {
    workspace_path("assets/replays").join(file_name)
}

pub fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}
