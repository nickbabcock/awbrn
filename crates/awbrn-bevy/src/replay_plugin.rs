use awbw_replay::{AwbwReplay, ReplayError, ReplayParser};
use bevy::app::{App, Plugin};
use bevy::asset::io::Reader;
use bevy::asset::{Asset, AssetApp, AssetLoader, LoadContext};
use bevy::prelude::TypePath;

/// Wrapper around AwbwReplay to make it a Bevy Asset
#[derive(Asset, TypePath, Debug, Clone)]
pub struct AwbwReplayAsset(pub AwbwReplay);

/// Plugin to register the AwbwReplayAsset and its loader
pub struct ReplayAssetPlugin;

impl Plugin for ReplayAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<AwbwReplayAsset>()
            .register_asset_loader(ReplayAssetLoader);
    }
}

/// Loads AwbwReplayAsset from .zip files
#[derive(Default)]
pub struct ReplayAssetLoader;

#[derive(Debug)]
pub enum ReplayAssetError {
    ReplayError(ReplayError),
    Io(std::io::Error),
}

impl From<std::io::Error> for ReplayAssetError {
    fn from(error: std::io::Error) -> Self {
        ReplayAssetError::Io(error)
    }
}

impl From<ReplayError> for ReplayAssetError {
    fn from(error: ReplayError) -> Self {
        ReplayAssetError::ReplayError(error)
    }
}

impl std::error::Error for ReplayAssetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReplayAssetError::Io(e) => Some(e),
            ReplayAssetError::ReplayError(e) => Some(e),
        }
    }
}

impl std::fmt::Display for ReplayAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayAssetError::Io(e) => write!(f, "IO error: {}", e),
            ReplayAssetError::ReplayError(e) => write!(f, "Replay error: {}", e),
        }
    }
}

impl AssetLoader for ReplayAssetLoader {
    type Asset = AwbwReplayAsset;
    type Settings = ();
    type Error = ReplayAssetError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        let parser = ReplayParser::new();
        let replay = parser.parse(&bytes)?;

        Ok(AwbwReplayAsset(replay))
    }

    fn extensions(&self) -> &[&str] {
        &["zip"]
    }
}
