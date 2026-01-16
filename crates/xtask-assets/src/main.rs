use anyhow::{Context, Result, anyhow};
use awbrn_types::{PlayerFaction, Unit};
use image::RgbaImage;
use indexmap::IndexMap;
use oxipng::{InFile, Options, OutFile};
use rectangle_pack::{
    GroupedRectsToPlace, PackedLocation, RectToInsert, TargetBin, contains_smallest_box,
    pack_rects, volume_heuristic,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use strum::VariantArray;
use walkdir::WalkDir;

const TILESHEET_COLUMNS: u32 = 64;
const UNITSHEET_COLUMNS: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WeatherKind {
    Clear,
    Snow,
    Rain,
}

impl WeatherKind {
    const ALL: [WeatherKind; 3] = [WeatherKind::Clear, WeatherKind::Snow, WeatherKind::Rain];

    fn as_rust(&self) -> &'static str {
        match self {
            WeatherKind::Clear => "Clear",
            WeatherKind::Snow => "Snow",
            WeatherKind::Rain => "Rain",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum FactionKey {
    Neutral,
    Player(String),
}

impl FactionKey {
    fn as_rust(&self) -> String {
        match self {
            FactionKey::Neutral => "Faction::Neutral".to_string(),
            FactionKey::Player(name) => format!("Faction::Player(PlayerFaction::{name})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum PropertyKind {
    Airport,
    Base,
    City,
    ComTower,
    HQ,
    Lab,
    Port,
}

impl PropertyKind {
    fn as_rust(&self) -> &'static str {
        match self {
            PropertyKind::Airport => "Airport",
            PropertyKind::Base => "Base",
            PropertyKind::City => "City",
            PropertyKind::ComTower => "ComTower",
            PropertyKind::HQ => "HQ",
            PropertyKind::Lab => "Lab",
            PropertyKind::Port => "Port",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum TerrainKey {
    StubbyMountain,
    Plain,
    Mountain,
    Wood,
    Reef,
    River(String),
    Road(String),
    Bridge(String),
    Sea(String),
    Shoal(String),
    Property {
        kind: PropertyKind,
        faction: FactionKey,
    },
    Pipe(String),
    PipeSeam(String),
    PipeRubble(String),
    MissileSilo(String),
    Teleporter,
}

impl TerrainKey {
    fn sort_key(&self) -> String {
        match self {
            TerrainKey::StubbyMountain => "0000-stubby".to_string(),
            TerrainKey::Plain => "0001-plain".to_string(),
            TerrainKey::Mountain => "0002-mountain".to_string(),
            TerrainKey::Wood => "0003-wood".to_string(),
            TerrainKey::Reef => "0004-reef".to_string(),
            TerrainKey::River(name) => format!("river-{name}"),
            TerrainKey::Road(name) => format!("road-{name}"),
            TerrainKey::Bridge(name) => format!("bridge-{name}"),
            TerrainKey::Sea(name) => format!("sea-{name}"),
            TerrainKey::Shoal(name) => format!("shoal-{name}"),
            TerrainKey::Property { kind, faction } => format!("property-{:?}-{:?}", kind, faction),
            TerrainKey::Pipe(name) => format!("pipe-{name}"),
            TerrainKey::PipeSeam(name) => format!("pipeseam-{name}"),
            TerrainKey::PipeRubble(name) => format!("piperubble-{name}"),
            TerrainKey::MissileSilo(name) => format!("missilesilo-{name}"),
            TerrainKey::Teleporter => "teleporter".to_string(),
        }
    }

    fn rust_pattern(&self) -> String {
        match self {
            TerrainKey::StubbyMountain => "GraphicalTerrain::StubbyMoutain".to_string(),
            TerrainKey::Plain => "GraphicalTerrain::Plain".to_string(),
            TerrainKey::Mountain => "GraphicalTerrain::Mountain".to_string(),
            TerrainKey::Wood => "GraphicalTerrain::Wood".to_string(),
            TerrainKey::Reef => "GraphicalTerrain::Reef".to_string(),
            TerrainKey::River(name) => format!("GraphicalTerrain::River(RiverType::{name})"),
            TerrainKey::Road(name) => format!("GraphicalTerrain::Road(RoadType::{name})"),
            TerrainKey::Bridge(name) => format!("GraphicalTerrain::Bridge(BridgeType::{name})"),
            TerrainKey::Sea(name) => format!("GraphicalTerrain::Sea(SeaDirection::{name})"),
            TerrainKey::Shoal(name) => format!("GraphicalTerrain::Shoal(ShoalDirection::{name})"),
            TerrainKey::Property { kind, faction } => match kind {
                PropertyKind::HQ => match faction {
                    FactionKey::Player(name) => {
                        format!("GraphicalTerrain::Property(Property::HQ(PlayerFaction::{name}))")
                    }
                    FactionKey::Neutral => {
                        panic!("Neutral HQ is not supported when generating spritesheet")
                    }
                },
                _ => format!(
                    "GraphicalTerrain::Property(Property::{}({}))",
                    kind.as_rust(),
                    faction.as_rust()
                ),
            },
            TerrainKey::Pipe(name) => format!("GraphicalTerrain::Pipe(PipeType::{name})"),
            TerrainKey::PipeSeam(name) => {
                format!("GraphicalTerrain::PipeSeam(PipeSeamType::{name})")
            }
            TerrainKey::PipeRubble(name) => {
                format!("GraphicalTerrain::PipeRubble(PipeRubbleType::{name})")
            }
            TerrainKey::MissileSilo(name) => {
                format!("GraphicalTerrain::MissileSilo(MissileSiloStatus::{name})")
            }
            TerrainKey::Teleporter => "GraphicalTerrain::Teleporter".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
enum TextureSource {
    Classic,
    Aw2,
    Custom(PathBuf),
}

#[derive(Debug, Clone)]
struct WeatherTexture {
    texture_key: String,
    source: TextureSource,
}

#[derive(Debug, Clone)]
struct WeatherTextures {
    clear: WeatherTexture,
    snow: Option<WeatherTexture>,
    rain: Option<WeatherTexture>,
}

#[derive(Debug, Clone)]
struct TileMetadata {
    awbw_id: u16,
    terrain: TerrainKey,
    textures: WeatherTextures,
    frames: u8,
    frame_timings: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SpriteIndex {
    start_index: u16,
    frames: u8,
}

#[derive(Debug)]
struct UiSprite {
    name: String,
    image: RgbaImage,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize)]
struct UiAtlasSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize)]
struct UiAtlasSprite {
    name: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize)]
struct UiAtlasData {
    size: UiAtlasSize,
    sprites: Vec<UiAtlasSprite>,
}

#[derive(Debug, Deserialize)]
struct TextureSet {
    #[serde(rename = "Clear")]
    clear: String,
    #[serde(rename = "Rain")]
    rain: Option<String>,
    #[serde(rename = "Snow")]
    snow: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TileEntry {
    #[serde(rename = "AWBWID")]
    awbw_id: u16,
    #[serde(rename = "TerrainType")]
    _terrain_type: String,
    #[serde(rename = "Textures")]
    textures: TextureSet,
}

#[derive(Debug, Deserialize)]
struct BuildingEntry {
    #[serde(rename = "AWBWID")]
    awbw_id: u16,
    #[serde(rename = "BuildingType")]
    building_type: String,
    #[serde(rename = "CountryID")]
    country_id: Option<u16>,
    #[serde(default, rename = "frames")]
    frames: Vec<u32>,
    #[serde(rename = "Textures")]
    textures: TextureSet,
}

#[derive(Debug, Deserialize, Clone)]
struct UnitAnimationEntry {
    #[serde(rename = "Texture")]
    texture: String,
    #[serde(rename = "Frames")]
    frames: Vec<u16>,
}

#[derive(Debug, Deserialize, Clone)]
struct UnitEntry {
    #[serde(rename = "Name")]
    _name: String,
    #[serde(rename = "IdleAnimation")]
    idle: UnitAnimationEntry,
    #[serde(rename = "MoveUpAnimation")]
    move_up: UnitAnimationEntry,
    #[serde(rename = "MoveDownAnimation")]
    move_down: UnitAnimationEntry,
    #[serde(rename = "MoveSideAnimation")]
    move_side: UnitAnimationEntry,
}

#[derive(Debug, Clone)]
struct UnitDefinition {
    unit: Unit,
    idle: UnitAnimationEntry,
    move_up: UnitAnimationEntry,
    move_down: UnitAnimationEntry,
    move_side: UnitAnimationEntry,
}

#[derive(Debug, Clone, Copy)]
struct FactionDefinition {
    faction: PlayerFaction,
    folder: &'static str,
}

const UNIT_FACTIONS: [FactionDefinition; 20] = [
    FactionDefinition {
        faction: PlayerFaction::AcidRain,
        folder: "AcidRain",
    },
    FactionDefinition {
        faction: PlayerFaction::AmberBlaze,
        folder: "AmberBlossom",
    },
    FactionDefinition {
        faction: PlayerFaction::AzureAsteroid,
        folder: "AzureAsteroid",
    },
    FactionDefinition {
        faction: PlayerFaction::BlackHole,
        folder: "BlackHole",
    },
    FactionDefinition {
        faction: PlayerFaction::BlueMoon,
        folder: "BlueMoon",
    },
    FactionDefinition {
        faction: PlayerFaction::BrownDesert,
        folder: "BrownDesert",
    },
    FactionDefinition {
        faction: PlayerFaction::CobaltIce,
        folder: "CobaltIce",
    },
    FactionDefinition {
        faction: PlayerFaction::GreenEarth,
        folder: "GreenEarth",
    },
    FactionDefinition {
        faction: PlayerFaction::GreySky,
        folder: "GreySky",
    },
    FactionDefinition {
        faction: PlayerFaction::JadeSun,
        folder: "JadeSun",
    },
    FactionDefinition {
        faction: PlayerFaction::NoirEclipse,
        folder: "NoirEclipse",
    },
    FactionDefinition {
        faction: PlayerFaction::OrangeStar,
        folder: "OrangeStar",
    },
    FactionDefinition {
        faction: PlayerFaction::PinkCosmos,
        folder: "PinkCosmos",
    },
    FactionDefinition {
        faction: PlayerFaction::PurpleLightning,
        folder: "PurpleLightning",
    },
    FactionDefinition {
        faction: PlayerFaction::RedFire,
        folder: "RedFire",
    },
    FactionDefinition {
        faction: PlayerFaction::SilverClaw,
        folder: "SilverClaw",
    },
    FactionDefinition {
        faction: PlayerFaction::TealGalaxy,
        folder: "TealGalaxy",
    },
    FactionDefinition {
        faction: PlayerFaction::UmberWilds,
        folder: "UmberWilds",
    },
    FactionDefinition {
        faction: PlayerFaction::WhiteNova,
        folder: "WhiteNova",
    },
    FactionDefinition {
        faction: PlayerFaction::YellowComet,
        folder: "YellowComet",
    },
];

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("tiles") => run_tiles(),
        Some("units") => run_units(),
        Some("ui") => run_ui(),
        _ => {
            eprintln!(
                "Usage: {} [tiles|units|ui]",
                args.first().map(String::as_str).unwrap_or("xtask-assets")
            );
            std::process::exit(1);
        }
    }
}

fn run_tiles() -> Result<()> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let assets_root = repo_root.join("assets/AWBW-Replay-Player/AWBWApp.Resources");
    let tiles_path = assets_root.join("Json/Tiles.json");
    let buildings_path = assets_root.join("Json/Buildings.json");
    let textures_classic = assets_root.join("Textures/Map/Classic");
    let textures_aw2 = assets_root.join("Textures/Map/AW2");
    let stubby_path = repo_root.join("assets/textures/stubby.png");
    let stubby_snow_path = repo_root.join("assets/textures/stubby-snow.png");
    let tilesheet_path = repo_root.join("assets/textures/tiles.png");
    let generated_dir = repo_root.join("crates/awbrn-core/src/generated");

    let tiles_map: BTreeMap<String, TileEntry> = load_json_map(&tiles_path)?;
    let buildings_map: BTreeMap<String, BuildingEntry> = load_json_map(&buildings_path)?;

    let mut tiles = Vec::new();

    for (name, entry) in tiles_map {
        let terrain = terrain_from_tile(&name, &entry)?;
        tiles.push(TileMetadata {
            awbw_id: entry.awbw_id,
            terrain,
            textures: WeatherTextures {
                clear: WeatherTexture {
                    texture_key: entry.textures.clear,
                    source: TextureSource::Classic,
                },
                snow: entry.textures.snow.map(|texture_key| WeatherTexture {
                    texture_key,
                    source: TextureSource::Classic,
                }),
                rain: entry.textures.rain.map(|texture_key| WeatherTexture {
                    texture_key,
                    source: TextureSource::Classic,
                }),
            },
            frames: 1,
            frame_timings: vec![300],
        });
    }

    for (name, entry) in buildings_map {
        let Some(terrain) = terrain_from_building(&name, &entry)? else {
            continue;
        };
        let frames = if entry.frames.is_empty() {
            1
        } else {
            entry
                .frames
                .len()
                .try_into()
                .map_err(|_| anyhow!("Animation frames exceed 255 for {name}"))?
        };

        let frame_timings = if entry.frames.is_empty() {
            vec![300] // Default for non-animated
        } else {
            entry.frames.clone()
        };

        tiles.push(TileMetadata {
            awbw_id: entry.awbw_id,
            terrain,
            textures: WeatherTextures {
                clear: WeatherTexture {
                    texture_key: entry.textures.clear,
                    source: TextureSource::Aw2,
                },
                snow: entry.textures.snow.map(|texture_key| WeatherTexture {
                    texture_key,
                    source: TextureSource::Aw2,
                }),
                rain: entry.textures.rain.map(|texture_key| WeatherTexture {
                    texture_key,
                    source: TextureSource::Aw2,
                }),
            },
            frames,
            frame_timings,
        });
    }

    tiles.push(TileMetadata {
        awbw_id: 0,
        terrain: TerrainKey::StubbyMountain,
        textures: WeatherTextures {
            clear: WeatherTexture {
                texture_key: "stubby".to_string(),
                source: TextureSource::Custom(stubby_path),
            },
            snow: Some(WeatherTexture {
                texture_key: "stubby-snow".to_string(),
                source: TextureSource::Custom(stubby_snow_path),
            }),
            rain: None,
        },
        frames: 1,
        frame_timings: vec![300],
    });

    let mut terrain_map = HashMap::new();
    for tile in tiles {
        let terrain = tile.terrain.clone();
        if terrain_map.insert(terrain.clone(), tile).is_some() {
            return Err(anyhow!("Duplicate terrain mapping for {:?}", terrain));
        }
    }

    add_sea_alias(&mut terrain_map, "E_S", "S_E")?;
    add_sea_alias(&mut terrain_map, "E_W", "W_E")?;

    let mut ordered_tiles: Vec<TileMetadata> = terrain_map.values().cloned().collect();
    ordered_tiles.sort_by(|a, b| {
        a.awbw_id
            .cmp(&b.awbw_id)
            .then_with(|| a.terrain.sort_key().cmp(&b.terrain.sort_key()))
    });

    let mut sprite_indices: HashMap<(TerrainKey, WeatherKind), SpriteIndex> = HashMap::new();
    let mut clear_frames = Vec::new();
    let mut snow_frames = Vec::new();
    let mut rain_frames = Vec::new();

    let mut clear_index: u16 = 0;
    for tile in &ordered_tiles {
        let sprite = SpriteIndex {
            start_index: clear_index,
            frames: tile.frames,
        };
        sprite_indices.insert((tile.terrain.clone(), WeatherKind::Clear), sprite);
        clear_index += tile.frames as u16;
        add_frames(
            &mut clear_frames,
            &textures_classic,
            &textures_aw2,
            tile,
            WeatherKind::Clear,
        )?;
    }

    let mut snow_index = clear_index;
    for tile in &ordered_tiles {
        if tile.textures.snow.is_some() {
            let sprite = SpriteIndex {
                start_index: snow_index,
                frames: tile.frames,
            };
            sprite_indices.insert((tile.terrain.clone(), WeatherKind::Snow), sprite);
            snow_index += tile.frames as u16;
            add_frames(
                &mut snow_frames,
                &textures_classic,
                &textures_aw2,
                tile,
                WeatherKind::Snow,
            )?;
        }
    }

    let mut rain_index = snow_index;
    for tile in &ordered_tiles {
        if tile.textures.rain.is_some() {
            let sprite = SpriteIndex {
                start_index: rain_index,
                frames: tile.frames,
            };
            sprite_indices.insert((tile.terrain.clone(), WeatherKind::Rain), sprite);
            rain_index += tile.frames as u16;
            add_frames(
                &mut rain_frames,
                &textures_classic,
                &textures_aw2,
                tile,
                WeatherKind::Rain,
            )?;
        }
    }

    let mut all_frames = Vec::new();
    all_frames.extend(clear_frames);
    all_frames.extend(snow_frames);
    all_frames.extend(rain_frames);

    build_spritesheet(&all_frames, &tilesheet_path, TILESHEET_COLUMNS)?;
    optimize_png(&tilesheet_path)?;

    let tilesheet_rows = (all_frames.len() as u32).div_ceil(TILESHEET_COLUMNS);

    fs::create_dir_all(&generated_dir).context("Creating generated output directory")?;
    let spritesheet_rs = generated_dir.join("spritesheet_index.rs");
    let spritesheet_contents = render_spritesheet_index(
        &ordered_tiles,
        &sprite_indices,
        TILESHEET_COLUMNS,
        tilesheet_rows,
    );
    fs::write(&spritesheet_rs, spritesheet_contents).context("Writing spritesheet_index.rs")?;

    // Generate terrain animation data
    let terrain_anim_rs = generated_dir.join("terrain_animation_data.rs");
    let terrain_anim_contents = render_terrain_animation_data(&ordered_tiles);
    fs::write(&terrain_anim_rs, terrain_anim_contents)
        .context("Writing terrain_animation_data.rs")?;

    Ok(())
}

fn run_units() -> Result<()> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let assets_root = repo_root.join("assets/AWBW-Replay-Player/AWBWApp.Resources");
    let units_path = assets_root.join("Json/Units.json");
    let textures_root = assets_root.join("Textures/Units");
    let unitsheet_path = repo_root.join("assets/textures/units.png");
    let generated_dir = repo_root.join("crates/awbrn-core/src/generated");

    validate_unit_faction_order()?;

    let units_in_order = load_units_in_order(&units_path)?;
    let unit_definitions = build_unit_definitions(units_in_order)?;

    let unit_frame_paths = collect_unit_frames(&unit_definitions, &textures_root)?;
    build_spritesheet(&unit_frame_paths, &unitsheet_path, UNITSHEET_COLUMNS)?;
    optimize_png(&unitsheet_path)?;

    fs::create_dir_all(&generated_dir).context("Creating generated output directory")?;
    let units_rs = generated_dir.join("unit_animation_data.rs");
    let units_contents = render_unit_animation_data(&unit_definitions);
    fs::write(&units_rs, units_contents).context("Writing unit_animation_data.rs")?;

    Ok(())
}

fn run_ui() -> Result<()> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let assets_root = repo_root.join("assets/AWBW-Replay-Player/AWBWApp.Resources");
    let ui_root = assets_root.join("Textures/UI");
    let atlas_path = repo_root.join("assets/textures/ui.png");
    let data_path = repo_root.join("assets/data/ui_atlas.json");

    let sprites = collect_ui_sprites(&ui_root)?;
    let (atlas_width, atlas_height, placements) = pack_ui_sprites(&sprites)?;

    build_ui_atlas(
        &sprites,
        &placements,
        &atlas_path,
        atlas_width,
        atlas_height,
    )?;
    optimize_png(&atlas_path)?;

    write_ui_atlas_data(&sprites, &placements, atlas_width, atlas_height, &data_path)?;

    Ok(())
}

fn collect_ui_sprites(ui_root: &Path) -> Result<Vec<UiSprite>> {
    let mut sprites = Vec::new();

    for entry in WalkDir::new(ui_root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("png") {
            continue;
        }
        let relative = path
            .strip_prefix(ui_root)
            .with_context(|| format!("Resolving relative path for {}", path.display()))?;
        if relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == "Power")
        {
            continue;
        }
        if relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == "Health")
        {
            continue;
        }
        let name = relative.to_string_lossy().replace('\\', "/");

        let image = image::open(path).with_context(|| format!("Loading {}", path.display()))?;
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();

        sprites.push(UiSprite {
            name,
            image: rgba,
            width,
            height,
        });
    }

    sprites.sort_by(|a, b| a.name.cmp(&b.name));

    if sprites.is_empty() {
        return Err(anyhow!("No UI sprites found in {}", ui_root.display()));
    }

    Ok(sprites)
}

fn pack_ui_sprites(sprites: &[UiSprite]) -> Result<(u32, u32, HashMap<String, PackedLocation>)> {
    let total_area: u32 = sprites
        .iter()
        .map(|sprite| sprite.width * sprite.height)
        .sum();
    let max_width = sprites.iter().map(|sprite| sprite.width).max().unwrap_or(1);
    let max_height = sprites
        .iter()
        .map(|sprite| sprite.height)
        .max()
        .unwrap_or(1);
    let mut side = ((total_area as f64).sqrt().ceil() as u32).max(1);
    side = side.max(max_width).max(max_height);

    let mut rects_to_place: GroupedRectsToPlace<String, ()> = GroupedRectsToPlace::new();
    for sprite in sprites {
        rects_to_place.push_rect(
            sprite.name.clone(),
            None,
            RectToInsert::new(sprite.width, sprite.height, 1),
        );
    }

    loop {
        let mut target_bins = BTreeMap::new();
        target_bins.insert(0u32, TargetBin::new(side, side, 1));

        match pack_rects(
            &rects_to_place,
            &mut target_bins,
            &volume_heuristic,
            &contains_smallest_box,
        ) {
            Ok(result) => {
                let placements = result
                    .packed_locations()
                    .into_iter()
                    .map(|(name, (_bin_id, location))| (name.clone(), *location))
                    .collect();
                return Ok((side, side, placements));
            }
            Err(_) => {
                side += 1;
            }
        }
    }
}

fn build_ui_atlas(
    sprites: &[UiSprite],
    placements: &HashMap<String, PackedLocation>,
    output_path: &Path,
    width: u32,
    height: u32,
) -> Result<()> {
    let mut atlas = RgbaImage::new(width, height);

    for sprite in sprites {
        let placement = placements
            .get(&sprite.name)
            .ok_or_else(|| anyhow!("Missing placement for {}", sprite.name))?;
        image::imageops::overlay(
            &mut atlas,
            &sprite.image,
            placement.x().into(),
            placement.y().into(),
        );
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).context("Creating UI atlas output directory")?;
    }

    atlas
        .save(output_path)
        .with_context(|| format!("Saving UI atlas to {}", output_path.display()))?;

    Ok(())
}

fn write_ui_atlas_data(
    sprites: &[UiSprite],
    placements: &HashMap<String, PackedLocation>,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<()> {
    let mut entries = Vec::new();

    for sprite in sprites {
        let placement = placements
            .get(&sprite.name)
            .ok_or_else(|| anyhow!("Missing placement for {}", sprite.name))?;

        entries.push(UiAtlasSprite {
            name: sprite.name.clone(),
            x: placement.x(),
            y: placement.y(),
            width: sprite.width,
            height: sprite.height,
        });
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let data = UiAtlasData {
        size: UiAtlasSize { width, height },
        sprites: entries,
    };

    let content = serde_json::to_string_pretty(&data).context("Serializing UI atlas data")?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).context("Creating UI atlas data directory")?;
    }
    fs::write(output_path, content).context("Writing UI atlas data")?;

    Ok(())
}

fn load_json_map<T: DeserializeOwned>(path: &Path) -> Result<BTreeMap<String, T>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    let filtered = strip_json_comments(&content);
    serde_json::from_str(&filtered).with_context(|| format!("Parsing {}", path.display()))
}

fn validate_unit_faction_order() -> Result<()> {
    for (index, entry) in UNIT_FACTIONS.iter().enumerate() {
        let expected = index as u8;
        let actual = entry.faction.index();
        if actual != expected {
            return Err(anyhow!(
                "Unit faction order mismatch for {:?}: expected {expected}, got {actual}",
                entry.faction
            ));
        }
    }
    Ok(())
}

fn load_units_in_order(path: &Path) -> Result<Vec<(String, UnitEntry)>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    let filtered = strip_json_comments(&content);
    let entries: IndexMap<String, UnitEntry> =
        serde_json::from_str(&filtered).with_context(|| format!("Parsing {}", path.display()))?;

    Ok(entries.into_iter().collect())
}

fn build_unit_definitions(units: Vec<(String, UnitEntry)>) -> Result<Vec<UnitDefinition>> {
    let mut seen = HashSet::new();
    let mut definitions = Vec::new();

    for (name, entry) in units {
        let unit = Unit::from_awbw_name(&name)
            .ok_or_else(|| anyhow!("Unknown unit name in Units.json: {name}"))?;
        if !seen.insert(unit) {
            return Err(anyhow!("Duplicate unit definition for {name}"));
        }

        let total_frames = unit_total_frames(&entry);
        if total_frames > 16 {
            return Err(anyhow!("Unit {name} has {total_frames} frames (max 16)"));
        }

        definitions.push(UnitDefinition {
            unit,
            idle: entry.idle,
            move_up: entry.move_up,
            move_down: entry.move_down,
            move_side: entry.move_side,
        });
    }

    for unit in Unit::VARIANTS {
        if !seen.contains(unit) {
            return Err(anyhow!("Missing unit definition for {unit:?}"));
        }
    }

    Ok(definitions)
}

fn collect_unit_frames(units: &[UnitDefinition], textures_root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for faction in UNIT_FACTIONS {
        let faction_root = textures_root.join(faction.folder);
        for unit in units {
            for animation in unit_animations_in_order(unit) {
                for frame in 0..animation.frames.len() {
                    let path = resolve_unit_frame_path(&faction_root, animation, frame);
                    if !path.exists() {
                        return Err(anyhow!("Missing texture file {}", path.display()));
                    }
                    paths.push(path);
                }
            }
        }
    }

    Ok(paths)
}

fn unit_animations_in_order(unit: &UnitDefinition) -> [&UnitAnimationEntry; 4] {
    [&unit.idle, &unit.move_up, &unit.move_down, &unit.move_side]
}

fn resolve_unit_frame_path(
    base_dir: &Path,
    animation: &UnitAnimationEntry,
    frame: usize,
) -> PathBuf {
    if animation.frames.len() == 1 {
        let base_path = base_dir.join(format!("{}.png", animation.texture));
        if base_path.exists() {
            return base_path;
        }
        return base_dir.join(format!("{}-0.png", animation.texture));
    }

    base_dir.join(format!("{}-{}.png", animation.texture, frame))
}

fn unit_total_frames(entry: &UnitEntry) -> usize {
    entry.idle.frames.len()
        + entry.move_up.frames.len()
        + entry.move_down.frames.len()
        + entry.move_side.frames.len()
}

fn unit_definition_total_frames(entry: &UnitDefinition) -> usize {
    entry.idle.frames.len()
        + entry.move_up.frames.len()
        + entry.move_down.frames.len()
        + entry.move_side.frames.len()
}

fn render_unit_animation_data(units: &[UnitDefinition]) -> String {
    let mut output = String::new();
    output.push_str("// This file is @generated by xtask-assets.\n\n");
    output.push_str("fn get_animation_data(unit: Unit) -> (u16, UnitAnimationData) {\n");
    output.push_str("    match unit {\n");

    let mut offset: u16 = 0;
    for unit in units {
        let unit_name = format!("{:?}", unit.unit);
        let idle_frames = format_frame_list(&unit.idle.frames);
        let move_up_frames = format_frame_list(&unit.move_up.frames);
        let move_down_frames = format_frame_list(&unit.move_down.frames);
        let move_side_frames = format_frame_list(&unit.move_side.frames);

        output.push_str(&format!(
            "        Unit::{unit_name} => ({offset}, UnitAnimationData::new(\n"
        ));
        output.push_str(&format!("            &[{idle_frames}],\n"));
        output.push_str(&format!("            &[{move_up_frames}],\n"));
        output.push_str(&format!("            &[{move_down_frames}],\n"));
        output.push_str(&format!("            &[{move_side_frames}],\n"));
        output.push_str("        )),\n");

        let total_frames = unit_definition_total_frames(unit);
        offset = offset
            .checked_add(total_frames as u16)
            .unwrap_or_else(|| panic!("Unit animation frame count overflow"));
    }

    output.push_str("    }\n}\n\n");
    output.push_str("fn faction_index(faction: PlayerFaction) -> u16 {\n");
    output.push_str("    match faction {\n");
    for (index, entry) in UNIT_FACTIONS.iter().enumerate() {
        let faction_name = format!("{:?}", entry.faction);
        output.push_str(&format!(
            "        PlayerFaction::{faction_name} => {index},\n"
        ));
    }
    output.push_str("    }\n}\n\n");

    output.push_str("impl UnitAnimationData {\n");
    output.push_str(&format!("    pub const TOTAL_FRAMES: usize = {offset};\n"));
    output.push_str("}\n");
    output
}

fn format_frame_list(frames: &[u16]) -> String {
    frames
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn add_sea_alias(
    terrain_map: &mut HashMap<TerrainKey, TileMetadata>,
    source: &str,
    alias: &str,
) -> Result<()> {
    let source_key = TerrainKey::Sea(source.to_string());
    let alias_key = TerrainKey::Sea(alias.to_string());

    if terrain_map.contains_key(&alias_key) {
        return Ok(());
    }

    let source_tile = terrain_map
        .get(&source_key)
        .cloned()
        .ok_or_else(|| anyhow!("Missing sea texture for {source}"))?;

    let mut alias_tile = source_tile.clone();
    alias_tile.terrain = alias_key;
    terrain_map.insert(alias_tile.terrain.clone(), alias_tile);
    Ok(())
}

fn strip_json_comments(content: &str) -> String {
    let mut output = String::new();
    let mut in_block = false;

    for line in content.lines() {
        let mut remainder = line;
        loop {
            if in_block {
                if let Some(end) = remainder.find("*/") {
                    remainder = &remainder[end + 2..];
                    in_block = false;
                } else {
                    break;
                }
            } else if let Some(start) = remainder.find("/*") {
                let (before, rest) = remainder.split_at(start);
                let trimmed = if let Some((prefix, _)) = before.split_once("//") {
                    prefix
                } else {
                    before
                };
                output.push_str(trimmed);
                remainder = &rest[2..];
                in_block = true;
            } else {
                let trimmed = if let Some((prefix, _)) = remainder.split_once("//") {
                    prefix
                } else {
                    remainder
                };
                output.push_str(trimmed);
                break;
            }
        }
        output.push('\n');
    }

    output
}

fn terrain_from_tile(name: &str, entry: &TileEntry) -> Result<TerrainKey> {
    let texture = entry.textures.clear.as_str();
    if texture == "Plain" {
        return Ok(TerrainKey::Plain);
    }
    if texture == "Mountain" {
        return Ok(TerrainKey::Mountain);
    }
    if texture == "Wood" {
        return Ok(TerrainKey::Wood);
    }
    if texture == "Reef" {
        return Ok(TerrainKey::Reef);
    }
    if texture == "Teleporter" {
        return Ok(TerrainKey::Teleporter);
    }
    if texture.starts_with("River/") {
        let variant = river_or_road_variant(texture.trim_start_matches("River/"))?;
        return Ok(TerrainKey::River(variant));
    }
    if texture.starts_with("Road/") {
        let suffix = texture.trim_start_matches("Road/");
        if suffix.starts_with('H') && suffix.contains("Bridge") {
            return Ok(TerrainKey::Bridge("Horizontal".to_string()));
        }
        if suffix.starts_with('V') && suffix.contains("Bridge") {
            return Ok(TerrainKey::Bridge("Vertical".to_string()));
        }
        let variant = river_or_road_variant(suffix)?;
        return Ok(TerrainKey::Road(variant));
    }
    if texture.starts_with("Sea/") {
        let variant = texture.trim_start_matches("Sea/").replace('-', "_");
        return Ok(TerrainKey::Sea(variant));
    }
    if texture.starts_with("Shoal/") {
        let variant = texture.trim_start_matches("Shoal/").replace('-', "");
        return Ok(TerrainKey::Shoal(variant));
    }
    if texture.starts_with("Pipe/") {
        let variant = pipe_variant(texture.trim_start_matches("Pipe/"))?;
        return Ok(TerrainKey::Pipe(variant));
    }
    if texture.starts_with("Neutral/HRubble") {
        return Ok(TerrainKey::PipeRubble("Horizontal".to_string()));
    }
    if texture.starts_with("Neutral/VRubble") {
        return Ok(TerrainKey::PipeRubble("Vertical".to_string()));
    }
    if texture.starts_with("Neutral/SiloEmpty") {
        return Ok(TerrainKey::MissileSilo("Unloaded".to_string()));
    }

    Err(anyhow!(
        "Unrecognized tile entry {name} with texture {texture}"
    ))
}

fn terrain_from_building(name: &str, entry: &BuildingEntry) -> Result<Option<TerrainKey>> {
    let building_type = entry.building_type.as_str();
    match building_type {
        "City" | "Base" | "Airport" | "Port" | "HQ" | "ComTower" | "Lab" => {
            let kind = match building_type {
                "City" => PropertyKind::City,
                "Base" => PropertyKind::Base,
                "Airport" => PropertyKind::Airport,
                "Port" => PropertyKind::Port,
                "HQ" => PropertyKind::HQ,
                "ComTower" => PropertyKind::ComTower,
                "Lab" => PropertyKind::Lab,
                _ => return Err(anyhow!("Unknown property type {building_type}")),
            };

            let faction = match entry.country_id {
                Some(id) => match player_faction_from_awbw_id(id) {
                    Some(name) => FactionKey::Player(name.to_string()),
                    None => {
                        eprintln!("Skipping building {name} with unsupported country id {id}");
                        return Ok(None);
                    }
                },
                None => FactionKey::Neutral,
            };

            if matches!(kind, PropertyKind::HQ) && matches!(faction, FactionKey::Neutral) {
                return Err(anyhow!("HQ cannot be neutral for {name}"));
            }

            Ok(Some(TerrainKey::Property { kind, faction }))
        }
        "Missile" => Ok(Some(TerrainKey::MissileSilo("Loaded".to_string()))),
        "PipeSeam" => {
            let suffix = entry.textures.clear.as_str();
            if suffix.contains("HSeam") {
                return Ok(Some(TerrainKey::PipeSeam("Horizontal".to_string())));
            }
            if suffix.contains("VSeam") {
                return Ok(Some(TerrainKey::PipeSeam("Vertical".to_string())));
            }
            Err(anyhow!("Unknown pipe seam texture for {name}"))
        }
        other => Err(anyhow!("Unsupported building type {other} for {name}")),
    }
}

fn river_or_road_variant(suffix: &str) -> Result<String> {
    match suffix {
        "H" => Ok("Horizontal".to_string()),
        "V" => Ok("Vertical".to_string()),
        "C" => Ok("Cross".to_string()),
        "ES" | "SW" | "WN" | "NE" | "ESW" | "SWN" | "WNE" | "NES" => Ok(suffix.to_string()),
        other => Err(anyhow!("Unknown river/road variant {other}")),
    }
}

fn pipe_variant(suffix: &str) -> Result<String> {
    match suffix {
        "V" => Ok("Vertical".to_string()),
        "H" => Ok("Horizontal".to_string()),
        "NE" | "ES" | "SW" | "WN" => Ok(suffix.to_string()),
        "NEnd" => Ok("NorthEnd".to_string()),
        "EEnd" => Ok("EastEnd".to_string()),
        "SEnd" => Ok("SouthEnd".to_string()),
        "WEnd" => Ok("WestEnd".to_string()),
        other => Err(anyhow!("Unknown pipe variant {other}")),
    }
}

fn player_faction_from_awbw_id(id: u16) -> Option<&'static str> {
    match id {
        1 => Some("OrangeStar"),
        2 => Some("BlueMoon"),
        3 => Some("GreenEarth"),
        4 => Some("YellowComet"),
        5 => Some("BlackHole"),
        6 => Some("RedFire"),
        7 => Some("GreySky"),
        8 => Some("BrownDesert"),
        9 => Some("AmberBlaze"),
        10 => Some("JadeSun"),
        16 => Some("CobaltIce"),
        17 => Some("PinkCosmos"),
        19 => Some("TealGalaxy"),
        20 => Some("PurpleLightning"),
        21 => Some("AcidRain"),
        22 => Some("WhiteNova"),
        23 => Some("AzureAsteroid"),
        24 => Some("NoirEclipse"),
        25 => Some("SilverClaw"),
        26 => Some("UmberWilds"),
        _ => None,
    }
}

fn add_frames(
    output: &mut Vec<PathBuf>,
    classic_root: &Path,
    aw2_root: &Path,
    tile: &TileMetadata,
    weather: WeatherKind,
) -> Result<()> {
    let texture = match weather {
        WeatherKind::Clear => &tile.textures.clear,
        WeatherKind::Snow => tile
            .textures
            .snow
            .as_ref()
            .ok_or_else(|| anyhow!("Missing snow texture"))?,
        WeatherKind::Rain => tile
            .textures
            .rain
            .as_ref()
            .ok_or_else(|| anyhow!("Missing rain texture"))?,
    };

    for frame in 0..tile.frames {
        let path = resolve_texture_path(texture, classic_root, aw2_root, frame, tile.frames)?;
        if !path.exists() {
            return Err(anyhow!("Missing texture file {}", path.display()));
        }
        output.push(path);
    }

    Ok(())
}

fn resolve_texture_path(
    texture: &WeatherTexture,
    classic_root: &Path,
    aw2_root: &Path,
    frame: u8,
    frames: u8,
) -> Result<PathBuf> {
    match &texture.source {
        TextureSource::Classic => {
            build_frame_path(classic_root, &texture.texture_key, frame, frames)
        }
        TextureSource::Aw2 => build_frame_path(aw2_root, &texture.texture_key, frame, frames),
        TextureSource::Custom(path) => {
            if frames != 1 {
                return Err(anyhow!(
                    "Custom texture {} cannot be animated",
                    path.display()
                ));
            }
            Ok(path.clone())
        }
    }
}

fn build_frame_path(base_dir: &Path, texture_key: &str, frame: u8, frames: u8) -> Result<PathBuf> {
    let has_numeric_suffix = texture_key
        .rsplit_once('-')
        .and_then(|(_, suffix)| suffix.parse::<u32>().ok())
        .is_some();

    if frames == 1 {
        let base_path = base_dir.join(format!("{texture_key}.png"));
        if base_path.exists() {
            return Ok(base_path);
        }
        if !has_numeric_suffix {
            let fallback = base_dir.join(format!("{texture_key}-0.png"));
            if fallback.exists() {
                return Ok(fallback);
            }
        }
        Ok(base_path)
    } else {
        if has_numeric_suffix {
            return Err(anyhow!(
                "Animated texture key already has numeric suffix: {texture_key}"
            ));
        }
        Ok(base_dir.join(format!("{texture_key}-{frame}.png")))
    }
}

fn build_spritesheet(paths: &[PathBuf], output_path: &Path, columns: u32) -> Result<()> {
    let mut images = Vec::new();
    let mut max_width = 0;
    let mut max_height = 0;

    for path in paths {
        let image = image::open(path).with_context(|| format!("Loading {}", path.display()))?;
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        max_width = max_width.max(width);
        max_height = max_height.max(height);
        images.push((path.clone(), rgba));
    }

    if images.is_empty() {
        return Err(anyhow!("No sprites were collected for the spritesheet"));
    }

    let cols = columns.max(1);
    let rows = (images.len() as u32).div_ceil(cols).max(1);
    let sheet_width = cols * max_width;
    let sheet_height = rows * max_height;

    let mut sheet = RgbaImage::new(sheet_width, sheet_height);

    for (index, (_path, image)) in images.into_iter().enumerate() {
        let col = (index as u32) % cols;
        let row = (index as u32) / cols;
        let base_x = col * max_width;
        let base_y = row * max_height;
        let x_offset = max_width.saturating_sub(image.width());
        let y_offset = max_height.saturating_sub(image.height());
        let x = base_x + x_offset;
        let y = base_y + y_offset;
        image::imageops::overlay(&mut sheet, &image, x.into(), y.into());
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).context("Creating spritesheet output directory")?;
    }

    sheet
        .save(output_path)
        .with_context(|| format!("Saving spritesheet to {}", output_path.display()))?;

    Ok(())
}

fn optimize_png(path: &Path) -> Result<()> {
    let options = Options::from_preset(3);
    oxipng::optimize(
        &InFile::Path(path.to_path_buf()),
        &OutFile::Path {
            path: Some(path.to_path_buf()),
            preserve_attrs: false,
        },
        &options,
    )
    .map(|_| ())
    .with_context(|| format!("Optimizing png {}", path.display()))
}

fn render_spritesheet_index(
    tiles: &[TileMetadata],
    sprite_indices: &HashMap<(TerrainKey, WeatherKind), SpriteIndex>,
    tilesheet_columns: u32,
    tilesheet_rows: u32,
) -> String {
    let mut output = String::new();
    output.push_str("// This file is @generated by xtask-assets.\n\n");
    output.push_str(&format!(
        "pub const TILESHEET_COLUMNS: u32 = {tilesheet_columns};\n",
    ));
    output.push_str(&format!(
        "pub const TILESHEET_ROWS: u32 = {tilesheet_rows};\n\n"
    ));
    output.push_str("#[rustfmt::skip]\n");
    output.push_str(
        "pub const fn spritesheet_index(weather: Weather, terrain: GraphicalTerrain) -> SpritesheetIndex {\n",
    );
    output.push_str("    match terrain {\n");

    for tile in tiles {
        let pattern = tile.terrain.rust_pattern();
        output.push_str(&format!("        {pattern} => match weather {{\n"));
        for weather in WeatherKind::ALL {
            let sprite = sprite_index_for(sprite_indices, &tile.terrain, weather);
            output.push_str(&format!(
                "            Weather::{} => SpritesheetIndex::new({}, {}),\n",
                weather.as_rust(),
                sprite.start_index,
                sprite.frames
            ));
        }
        output.push_str("        },\n");
    }

    output.push_str("    }\n}\n");
    output
}

fn sprite_index_for(
    sprite_indices: &HashMap<(TerrainKey, WeatherKind), SpriteIndex>,
    terrain: &TerrainKey,
    weather: WeatherKind,
) -> SpriteIndex {
    sprite_indices
        .get(&(terrain.clone(), weather))
        .copied()
        .unwrap_or_else(|| {
            sprite_indices
                .get(&(terrain.clone(), WeatherKind::Clear))
                .copied()
                .expect("Missing clear texture")
        })
}

fn render_terrain_animation_data(tiles: &[TileMetadata]) -> String {
    let mut output = String::new();
    output.push_str("// This file is @generated by xtask-assets.\n\n");

    // Generate static arrays for each animated building's frame timings
    let mut const_counter = 0;
    let mut const_names = Vec::new();

    for tile in tiles {
        if tile.frames <= 1 {
            continue; // Skip static (non-animated) tiles
        }

        let const_name = format!("FRAMES_{}", const_counter);
        const_counter += 1;

        // Convert timings to u16 array literal
        let timings: Vec<String> = tile
            .frame_timings
            .iter()
            .map(|t| format!("{}", (*t as u16).min(65535)))
            .collect();

        output.push_str(&format!(
            "const {}: &[u16] = &[{}];\n",
            const_name,
            timings.join(", ")
        ));

        const_names.push((tile.terrain.rust_pattern(), const_name));
    }

    output.push_str("\n");
    output.push_str(
        "fn get_animation_timing(terrain: GraphicalTerrain) -> Option<TerrainAnimationFrames> {\n",
    );
    output.push_str("    match terrain {\n");

    // Generate match arms referencing the const arrays
    for (pattern, const_name) in const_names {
        output.push_str(&format!(
            "        {} => Some(TerrainAnimationFrames::new({const_name})),\n",
            pattern
        ));
    }

    output.push_str("        _ => None,\n");
    output.push_str("    }\n}\n");
    output
}
