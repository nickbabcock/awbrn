//! The generated tables, checked against the documents they were lowered from.
//!
//! `cargo xtask-ruleset --check` proves the checked-in file is what the
//! generator currently emits. That is a claim about the generator, not about
//! the specification. This test is the other half: it re-reads
//! `spec/rulesets/awbw/2026-07-10/**` as untyped JSON and asserts every value
//! AWVM will actually index still says what the document says.
//!
//! Between the two, a table cannot disagree with the specification without
//! something going red.

use awvm::ruleset::{
    self, AMMO_DAMAGE, Command, ConcealmentMode, Domain, DrawReason, FireMode, KnownReason,
    MOVEMENT_COSTS, MovementClass, PropertyKind, RULESET_ID, RULESET_REVISION, Relation, Resource,
    ResourceSet, SupplyTrigger, TERRAIN_PROFILES, TargetSet, Terrain, TerrainTrait, UNIT_PROFILES,
    UNLIMITED_DAMAGE, UnitKind, VictoryReason, WEAPON_SELECTION_ORDER,
    WEAPON_SELECTION_REQUIRES_AVAILABLE_AMMO, WeaponPolicy, WeaponSlot, WeatherKind,
};
use serde_json::Value;

fn document(name: &str) -> Value {
    let raw = match name {
        "units" => include_str!("../../../spec/rulesets/awbw/2026-07-10/units.json"),
        "terrain" => include_str!("../../../spec/rulesets/awbw/2026-07-10/terrain.json"),
        "movement-costs" => {
            include_str!("../../../spec/rulesets/awbw/2026-07-10/movement-costs.json")
        }
        "unit-capabilities" => {
            include_str!("../../../spec/rulesets/awbw/2026-07-10/unit-capabilities.json")
        }
        "combat-profiles" => {
            include_str!("../../../spec/rulesets/awbw/2026-07-10/combat-profiles.json")
        }
        "weapons" => include_str!("../../../spec/rulesets/awbw/2026-07-10/weapons.json"),
        "commander-profiles" => {
            include_str!("../../../spec/rulesets/awbw/2026-07-10/commander-profiles.json")
        }
        "reasons" => include_str!("../../../spec/rulesets/awbw/2026-07-10/reasons.json"),
        other => panic!("no such ruleset document: {other}"),
    };
    serde_json::from_str(raw).expect("ruleset documents are valid JSON")
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("expected a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// A vocabulary enum must cover its document's keys exactly. Missing variants
/// mean silent lookup failures; extra ones mean the tables carry rules the
/// specification does not.
#[test]
fn vocabularies_match_their_documents() {
    assert_eq!(
        UnitKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_owned())
            .collect::<Vec<_>>(),
        {
            let mut names = keys(&document("units")["units"]);
            names.sort();
            names
        }
    );
    assert_eq!(
        Terrain::ALL
            .iter()
            .map(|terrain| terrain.as_str().to_owned())
            .collect::<Vec<_>>(),
        {
            let mut names = keys(&document("terrain")["terrains"]);
            names.sort();
            names
        }
    );

    let movement = document("movement-costs");
    assert_eq!(
        MovementClass::ALL
            .iter()
            .map(|class| class.as_str())
            .collect::<Vec<_>>(),
        movement["movement_classes"]
            .as_array()
            .expect("movement classes are an array")
            .iter()
            .map(|class| class.as_str().expect("movement classes are strings"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        WeatherKind::ALL
            .iter()
            .map(|weather| weather.as_str())
            .collect::<Vec<_>>(),
        movement["weather"]
            .as_array()
            .expect("weather is an array")
            .iter()
            .map(|weather| weather.as_str().expect("weather kinds are strings"))
            .collect::<Vec<_>>()
    );

    // The commander vocabulary is generated but the commander tables are still
    // parsed, so nothing else would catch a commander being added.
    let mut commanders = keys(&document("commander-profiles")["commanders"]);
    commanders.sort();
    assert_eq!(
        ruleset::CommanderKind::ALL
            .iter()
            .map(|commander| commander.as_str().to_owned())
            .collect::<Vec<_>>(),
        commanders
    );

    let reasons = document("reasons");
    for (name, generated) in [
        (
            "known",
            KnownReason::ALL
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "victory",
            VictoryReason::ALL
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "draw",
            DrawReason::ALL
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>(),
        ),
    ] {
        assert_eq!(
            generated,
            reasons[name]
                .as_array()
                .expect("reason vocabulary is an array")
                .iter()
                .map(|reason| reason.as_str().expect("reasons are strings"))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn revision_matches_the_directory_the_tables_came_from() {
    assert_eq!(RULESET_ID, "awbw");
    assert_eq!(RULESET_REVISION, "2026-07-10");
}

#[test]
fn unit_profiles_match_units_json() {
    let units = document("units");
    for kind in UnitKind::ALL {
        let entry = &units["units"][kind.as_str()];
        let profile = ruleset::profile(kind);

        assert_eq!(profile.kind, kind, "{kind} is stored at the wrong index");
        assert_eq!(profile.awbw_id, entry["awbw_id"].as_u64().unwrap() as u32);
        assert_eq!(profile.domain.as_str(), entry["domain"].as_str().unwrap());
        assert_eq!(profile.cost, entry["cost"].as_u64().unwrap());
        assert_eq!(profile.movement, entry["move"].as_u64().unwrap());
        assert_eq!(
            profile.movement_class.as_str(),
            entry["movement_class"].as_str().unwrap()
        );
        assert_eq!(profile.max_fuel, entry["max_fuel"].as_u64().unwrap());
        assert_eq!(profile.max_ammo, entry["max_ammo"].as_u64().unwrap());
        assert_eq!(
            profile.fuel_per_turn.normal,
            entry["fuel_per_turn"]["normal"].as_u64().unwrap()
        );
        assert_eq!(
            profile.fuel_per_turn.hidden,
            entry["fuel_per_turn"]["hidden"].as_u64()
        );
        assert_eq!(profile.vision, entry["vision"].as_i64().unwrap());
        assert_eq!(
            profile
                .indirect_range
                .map(|range| (range.minimum, range.maximum)),
            entry["indirect_range"].as_object().map(|range| (
                range["min"].as_u64().unwrap(),
                range["max"].as_u64().unwrap()
            ))
        );
    }
}

#[test]
fn combat_profiles_are_merged_into_unit_profiles() {
    let profiles = document("combat-profiles");
    for kind in UnitKind::ALL {
        let entry = &profiles["units"][kind.as_str()];
        let profile = ruleset::profile(kind);
        assert_eq!(
            profile.fire_mode.as_str(),
            entry["fire_mode"].as_str().unwrap(),
            "{kind} fire mode"
        );
        assert_eq!(
            profile.weapon_policy.as_str(),
            entry["weapon_policy"].as_str().unwrap(),
            "{kind} weapon policy"
        );
    }
}

#[test]
fn damage_matrices_match_weapons_json() {
    let weapons = document("weapons");

    assert_eq!(
        WEAPON_SELECTION_ORDER
            .iter()
            .map(|slot| slot.as_str())
            .collect::<Vec<_>>(),
        weapons["selection"]["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slot| slot.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        WEAPON_SELECTION_REQUIRES_AVAILABLE_AMMO,
        weapons["selection"]["requires_available_ammo"]
            .as_bool()
            .unwrap()
    );

    for slot in WEAPON_SELECTION_ORDER {
        let table = match slot {
            WeaponSlot::Ammo => &AMMO_DAMAGE,
            WeaponSlot::Unlimited => &UNLIMITED_DAMAGE,
        };
        for attacker in UnitKind::ALL {
            let entry = &weapons["units"][attacker.as_str()][slot.as_str()];
            let weapon = ruleset::profile(attacker).weapon(slot);

            assert_eq!(
                weapon.is_some(),
                entry.is_object(),
                "{attacker} {slot} weapon presence"
            );
            let Some(weapon) = weapon else {
                // A unit with no weapon in this slot must have an empty row,
                // so an index can never return damage the document denies.
                assert!(
                    table[attacker.index()].iter().all(Option::is_none),
                    "{attacker} has no {slot} weapon but its damage row is populated"
                );
                continue;
            };
            assert_eq!(weapon.slot, slot);
            assert_eq!(weapon.ammo_cost, entry["ammo_cost"].as_u64().unwrap());
            for defender in UnitKind::ALL {
                assert_eq!(
                    weapon.damage(defender).map(u64::from),
                    entry["damage"][defender.as_str()].as_u64(),
                    "{attacker} {slot} vs {defender}"
                );
                assert_eq!(
                    table[attacker.index()][defender.index()],
                    weapon.damage(defender),
                    "{attacker} {slot} row disagrees with its profile"
                );
            }
        }
    }
}

#[test]
fn capabilities_are_merged_into_unit_profiles() {
    let capabilities = document("unit-capabilities");
    let listed = |field: &str, kind: UnitKind| {
        capabilities[field]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some(kind.as_str()))
    };

    for kind in UnitKind::ALL {
        let profile = ruleset::profile(kind);
        assert_eq!(
            profile.can_capture,
            listed("capture", kind),
            "{kind} capture"
        );
        assert_eq!(
            profile.elevated_vision,
            listed("elevated_vision", kind),
            "{kind} elevated vision"
        );

        let transport = &capabilities["transport"][kind.as_str()];
        match profile.transport {
            Some(carried) => {
                assert_eq!(
                    carried.capacity,
                    transport["capacity"].as_u64().unwrap() as usize
                );
                for cargo in UnitKind::ALL {
                    let allowed = transport["cargo"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|value| value.as_str() == Some(cargo.as_str()));
                    assert_eq!(
                        carried.cargo.contains(cargo),
                        allowed,
                        "{kind} carrying {cargo}"
                    );
                }
            }
            None => assert!(transport.is_null(), "{kind} transport"),
        }

        let supply = &capabilities["supply"][kind.as_str()];
        match profile.supply {
            Some(supplies) => {
                assert_eq!(
                    supplies.trigger.as_str(),
                    supply["trigger"].as_str().unwrap()
                );
                assert_eq!(
                    supplies.relation.as_str(),
                    supply["relation"].as_str().unwrap()
                );
                assert_eq!(
                    supplies.targets.as_str(),
                    supply["targets"].as_str().unwrap()
                );
                assert_eq!(supplies.refill, resource_set(&supply["refill"]));
            }
            None => assert!(supply.is_null(), "{kind} supply"),
        }

        let repair = &capabilities["repair"][kind.as_str()];
        match profile.repair {
            Some(repairs) => {
                assert_eq!(
                    repairs.command.as_str(),
                    repair["command"].as_str().unwrap()
                );
                assert_eq!(
                    repairs.relation.as_str(),
                    repair["relation"].as_str().unwrap()
                );
                assert_eq!(
                    repairs.targets.as_str(),
                    repair["targets"].as_str().unwrap()
                );
                assert_eq!(
                    u64::from(repairs.exact_hp),
                    repair["exact_hp"].as_u64().unwrap()
                );
                assert_eq!(
                    repairs.cost_percent,
                    repair["cost_percent"].as_u64().unwrap()
                );
                assert_eq!(repairs.also_refills, resource_set(&repair["also_refills"]));
            }
            None => assert!(repair.is_null(), "{kind} repair"),
        }

        let concealment = &capabilities["concealment"][kind.as_str()];
        match profile.concealment {
            Some(hiding) => {
                assert_eq!(hiding.mode.as_str(), concealment["mode"].as_str().unwrap());
                assert_eq!(
                    hiding.enter_command.as_str(),
                    concealment["enter_command"].as_str().unwrap()
                );
                assert_eq!(
                    hiding.exit_command.as_str(),
                    concealment["exit_command"].as_str().unwrap()
                );
            }
            None => assert!(concealment.is_null(), "{kind} concealment"),
        }

        let actions = &capabilities["special_actions"][kind.as_str()];
        assert_eq!(
            profile
                .special_actions
                .iter()
                .map(|action| action.as_str())
                .collect::<Vec<_>>(),
            actions
                .as_array()
                .map(|actions| actions
                    .iter()
                    .map(|action| action.as_str().unwrap())
                    .collect::<Vec<_>>())
                .unwrap_or_default(),
            "{kind} special actions"
        );
    }
}

fn resource_set(value: &Value) -> ResourceSet {
    let names: Vec<&str> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap())
        .collect();
    let resources: Vec<Resource> = names
        .iter()
        .map(|name| Resource::from_id(name).unwrap_or_else(|| panic!("unknown resource {name}")))
        .collect();
    ResourceSet::new(&resources)
}

#[test]
fn terrain_profiles_match_terrain_json() {
    let terrain = document("terrain");
    for kind in Terrain::ALL {
        let entry = &terrain["terrains"][kind.as_str()];
        let profile = ruleset::terrain(kind);

        assert_eq!(profile.terrain, kind, "{kind} is stored at the wrong index");
        assert_eq!(
            u64::from(profile.defense_stars),
            entry["defense_stars"].as_u64().unwrap()
        );
        assert_eq!(
            profile.property_kind.map(PropertyKind::as_str),
            entry["property_kind"].as_str()
        );
        assert_eq!(profile.vision_bonus, entry["vision_bonus"].as_i64());
        assert_eq!(
            profile.vision_limit.map(|limit| limit as u64),
            entry["vision_limit"].as_u64()
        );
        assert_eq!(
            profile.elimination_replacement.map(Terrain::as_str),
            entry["elimination_replacement"].as_str()
        );

        for value in TerrainTrait::ALL {
            let listed = entry["traits"]
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name.as_str() == Some(value.as_str()));
            assert_eq!(profile.has(value), listed, "{kind} trait {value}");
        }

        let destructible = &entry["destructible"];
        match profile.destructible {
            Some(destroyable) => {
                assert_eq!(
                    destroyable.maximum_hp,
                    destructible["maximum_hp"].as_u64().unwrap()
                );
                assert_eq!(
                    destroyable.target_kind.as_str(),
                    destructible["target_kind"].as_str().unwrap()
                );
                assert_eq!(
                    destroyable.destruction_replacement.as_str(),
                    destructible["destruction_replacement"].as_str().unwrap()
                );
            }
            None => assert!(destructible.is_null(), "{kind} destructible"),
        }
    }
}

#[test]
fn movement_costs_match_movement_costs_json() {
    let movement = document("movement-costs");
    for terrain in Terrain::ALL {
        for weather in WeatherKind::ALL {
            let column = movement["terrains"][terrain.as_str()][weather.as_str()]
                .as_object()
                .unwrap_or_else(|| panic!("movement-costs.json has no {terrain} in {weather}"));
            for class in MovementClass::ALL {
                let entry = column.get(class.as_str()).unwrap_or_else(|| {
                    panic!("movement-costs.json has no entry for {terrain}/{weather}/{class}")
                });
                assert_eq!(
                    MOVEMENT_COSTS[terrain.index()][weather.index()][class.index()].map(u64::from),
                    entry.as_u64(),
                    "{terrain} in {weather} for {class}"
                );
            }
        }
    }
}

#[test]
fn teleporters_cost_zero_for_every_weather_and_movement_class() {
    for weather in WeatherKind::ALL {
        for class in MovementClass::ALL {
            assert_eq!(
                ruleset::movement_cost(Terrain::Teleporter, weather, class),
                Some(0),
                "teleporter in {weather} for {class}"
            );
        }
    }
}

/// Every vocabulary round-trips through its wire identifier, so the enums can
/// stand in for the strings the model still carries.
#[test]
fn identifiers_round_trip() {
    macro_rules! round_trip {
        ($($kind:ty),+ $(,)?) => {
            $(
                for value in <$kind>::ALL {
                    assert_eq!(<$kind>::from_id(value.as_str()), Some(value));
                    assert_eq!(value.index(), <$kind>::ALL.iter().position(|other| *other == value).unwrap());
                }
                assert!(<$kind>::from_id("not-a-real-identifier").is_none());
            )+
        };
    }

    round_trip!(
        UnitKind,
        Terrain,
        MovementClass,
        WeatherKind,
        Domain,
        FireMode,
        WeaponPolicy,
        WeaponSlot,
        ruleset::CommanderKind,
        TerrainTrait,
        PropertyKind,
        Resource,
        SupplyTrigger,
        Relation,
        TargetSet,
        Command,
        ConcealmentMode,
        KnownReason,
        VictoryReason,
        DrawReason,
    );

    assert_eq!(UNIT_PROFILES.len(), UnitKind::COUNT);
    assert_eq!(TERRAIN_PROFILES.len(), Terrain::COUNT);
}
