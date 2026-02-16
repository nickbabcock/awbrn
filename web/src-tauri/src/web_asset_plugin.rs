use awbrn_bevy::MapAssetPathResolver;
use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetSourceBuilder, PathStream, Reader, VecReader,
};
use bevy::prelude::*;
use bevy::tasks::ConditionalSendFuture;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Default)]
pub struct WebAssetPlugin;

impl Plugin for WebAssetPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_source(
            "https",
            AssetSourceBuilder::new(move || Box::new(WebAssetReader)),
        );
    }
}

pub struct WebMapAssetPathResolver;

impl MapAssetPathResolver for WebMapAssetPathResolver {
    fn resolve_path(&self, map_id: u32) -> String {
        format!(
            "https://awbw.amarriner.com/api/map/map_info.php?maps_id={}",
            map_id
        )
    }
}

pub struct WebAssetReader;

impl WebAssetReader {
    fn uri(path: &Path) -> String {
        format!("https://{}", path.display())
    }

    async fn get(&self, path: &Path) -> Result<Box<dyn Reader>, AssetReaderError> {
        let uri = Self::uri(path);
        let request = ehttp::Request::get(&uri);
        let response = ehttp::fetch_async(request)
            .await
            .map_err(|error| AssetReaderError::Io(Arc::new(std::io::Error::other(error))))?;

        if !response.ok {
            return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                "Failed to fetch asset",
            ))));
        }

        Ok(Box::new(VecReader::new(response.bytes)))
    }
}

impl AssetReader for WebAssetReader {
    fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<dyn Reader>, AssetReaderError>> {
        Box::pin(self.get(path))
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<Box<dyn Reader>, AssetReaderError> {
        Err(AssetReaderError::NotFound(PathBuf::from(Self::uri(path))))
    }

    async fn is_directory<'a>(&'a self, _path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(false)
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        Err(AssetReaderError::NotFound(PathBuf::from(Self::uri(path))))
    }
}
