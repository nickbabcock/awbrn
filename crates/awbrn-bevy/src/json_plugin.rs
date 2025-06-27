use bevy::app::{App, Plugin};
use bevy::asset::{Asset, AssetApp, AssetLoader, LoadContext, io::Reader};
use serde_json::from_slice;
use std::marker::PhantomData;

pub struct JsonAssetPlugin<A> {
    _type: PhantomData<A>,
}

impl<A> Plugin for JsonAssetPlugin<A>
where
    for<'de> A: serde::Deserialize<'de> + Asset,
{
    fn build(&self, app: &mut App) {
        app.init_asset::<A>()
            .register_asset_loader(JsonAssetLoader::<A> { _type: PhantomData });
    }
}

impl<A> JsonAssetPlugin<A>
where
    for<'de> A: serde::Deserialize<'de> + Asset,
{
    /// Create a new plugin that will load assets from files with the given extensions.
    pub fn new() -> Self {
        Self { _type: PhantomData }
    }
}

impl<A> Default for JsonAssetPlugin<A>
where
    for<'de> A: serde::Deserialize<'de> + Asset,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Loads your asset type `A` from json files
pub struct JsonAssetLoader<A> {
    _type: PhantomData<A>,
}

#[derive(Debug)]
pub enum JsonAssetError {
    Io(std::io::Error),
    JsonError(serde_json::error::Error),
}

impl From<std::io::Error> for JsonAssetError {
    fn from(error: std::io::Error) -> Self {
        JsonAssetError::Io(error)
    }
}

impl From<serde_json::error::Error> for JsonAssetError {
    fn from(error: serde_json::error::Error) -> Self {
        JsonAssetError::JsonError(error)
    }
}

impl std::error::Error for JsonAssetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JsonAssetError::Io(e) => Some(e),
            JsonAssetError::JsonError(e) => Some(e),
        }
    }
}

impl std::fmt::Display for JsonAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonAssetError::Io(e) => write!(f, "IO error: {}", e),
            JsonAssetError::JsonError(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl<A> AssetLoader for JsonAssetLoader<A>
where
    for<'de> A: serde::Deserialize<'de> + Asset,
{
    type Asset = A;
    type Settings = ();
    type Error = JsonAssetError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let asset = from_slice::<A>(&bytes)?;
        Ok(asset)
    }

    fn extensions(&self) -> &[&str] {
        &["json"]
    }
}
