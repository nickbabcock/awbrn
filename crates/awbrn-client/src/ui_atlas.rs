use awbrn_content::UiAtlasManifest;
use bevy::math::{URect, UVec2};
use bevy::prelude::{Asset, TextureAtlasLayout, TypePath};
use serde::Deserialize;
use std::collections::HashMap;

pub use awbrn_content::{UiAtlasSize, UiAtlasSprite};

#[derive(Asset, TypePath, Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct UiAtlasAsset(pub UiAtlasManifest);

impl UiAtlasAsset {
    pub fn layout(&self) -> TextureAtlasLayout {
        let mut layout =
            TextureAtlasLayout::new_empty(UVec2::new(self.0.size.width, self.0.size.height));

        for sprite in &self.0.sprites {
            layout.textures.push(URect {
                min: UVec2::new(sprite.x, sprite.y),
                max: UVec2::new(sprite.x + sprite.width, sprite.y + sprite.height),
            });
        }

        layout
    }

    pub fn index_map(&self) -> HashMap<String, usize> {
        self.0
            .sprites
            .iter()
            .enumerate()
            .map(|(index, sprite)| (sprite.name.clone(), index))
            .collect()
    }
}
