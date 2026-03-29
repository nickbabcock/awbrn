use awbrn_client::MapAssetPathResolver;

pub(crate) struct WebMapAssetPathResolver;

impl MapAssetPathResolver for WebMapAssetPathResolver {
    fn resolve_path(&self, map_id: u32) -> String {
        format!(
            "https://awbw.amarriner.com/api/map/map_info.php?maps_id={}",
            map_id
        )
    }
}
