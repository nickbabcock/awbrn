use bevy::math::{URect, UVec2};
use bevy::prelude::{Asset, TextureAtlasLayout, TypePath};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct UiAtlasSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiAtlasSprite {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Asset, TypePath, Debug, Clone, Deserialize)]
pub struct UiAtlasAsset {
    pub size: UiAtlasSize,
    pub sprites: Vec<UiAtlasSprite>,
}

impl UiAtlasAsset {
    pub fn layout(&self) -> TextureAtlasLayout {
        let mut layout =
            TextureAtlasLayout::new_empty(UVec2::new(self.size.width, self.size.height));

        for sprite in &self.sprites {
            layout.textures.push(URect {
                min: UVec2::new(sprite.x, sprite.y),
                max: UVec2::new(sprite.x + sprite.width, sprite.y + sprite.height),
            });
        }

        layout
    }

    pub fn index_map(&self) -> HashMap<String, usize> {
        self.sprites
            .iter()
            .enumerate()
            .map(|(index, sprite)| (sprite.name.clone(), index))
            .collect()
    }
}
