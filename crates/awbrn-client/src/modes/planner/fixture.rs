//! The cowboy planner board for the planner tests.

use std::path::Path;

use awbrn_map::AwbwMapData;
use awbrn_types::{AwbwCoId, AwbwGamePlayerId, AwbwMapId, PlayerFaction};
use awbw_replay::AwbwReplay;
use awbw_replay::game_models::CoPower;
use awbw_replay::move_planner::{MovePlanner, PlannerPlayer, PlannerSeat, PlannerSetup};

pub(super) const RACHEL: AwbwCoId = AwbwCoId::new(28);
pub(super) const ADDER: AwbwCoId = AwbwCoId::new(11);
pub(super) const JESS: AwbwCoId = AwbwCoId::new(14);

/// Game 1710706 on map 186800.
pub(super) struct Cowboy {
    /// The Blue Moon player.
    pub player: AwbwGamePlayerId,
    pub players: Vec<PlannerPlayer>,
    pub replay: AwbwReplay,
    pub map: AwbwMapData,
}

/// The cowboy board with Blue Moon first, as `co_id`, with `stars` power
/// stars.
pub(super) fn cowboy(co_id: AwbwCoId, stars: u64) -> Cowboy {
    cowboy_seated(
        &[(PlayerFaction::BlueMoon, co_id, stars)],
        PlayerFaction::BlueMoon,
    )
}

/// The cowboy board with `first` first, and each `(army, CO, power stars)`
/// of `seats`. The other armies have no CO.
pub(super) fn cowboy_seated(
    seats: &[(PlayerFaction, AwbwCoId, u64)],
    first: PlayerFaction,
) -> Cowboy {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let planner =
        MovePlanner::from_slice(&std::fs::read(root.join("move_planners/cowboy.json")).unwrap())
            .unwrap();
    let players = planner.players();
    let id = |faction| {
        players
            .iter()
            .find(|player| player.faction == faction)
            .unwrap()
            .id
    };
    let star = awvm::commander::power_star_charge(0).unwrap();
    let replay = planner.to_replay(&PlannerSetup {
        map_id: AwbwMapId::new(186800),
        seats: seats
            .iter()
            .map(|&(faction, co_id, stars)| PlannerSeat {
                player: id(faction),
                co_id,
                team: None,
                funds: 0,
                // AWBW records ten times the charge that AWVM counts.
                co_power: u32::try_from(stars * star * 10).unwrap(),
                co_power_on: CoPower::None,
            })
            .collect(),
        first_player: Some(id(first)),
    });
    let map =
        serde_json::from_slice(&std::fs::read(root.join("maps/186800.json")).unwrap()).unwrap();
    Cowboy {
        player: id(PlayerFaction::BlueMoon),
        players,
        replay,
        map,
    }
}

impl Cowboy {
    pub fn state(&self) -> awvm::semantic::State {
        awvm_awbw::initial_state(&self.replay, &self.map).unwrap()
    }

    /// The player of `faction`.
    pub fn id(&self, faction: PlayerFaction) -> AwbwGamePlayerId {
        self.players
            .iter()
            .find(|player| player.faction == faction)
            .unwrap()
            .id
    }
}
