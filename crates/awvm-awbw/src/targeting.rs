//! Recipient-targeted payload lookups shared by the compatibility and
//! recorded-outcome reducers.
//!
//! AWBW writes one row per recipient. The globally visible row is the
//! authoritative one when present; otherwise any recipient row that discloses
//! the value is the best available evidence of what happened.

use awbw_replay::Hidden;
use awbw_replay::turn_models::{TargetedPlayer, UnitMap, UnitProperty};
use indexmap::IndexMap;

pub(crate) fn visible_unit(units: &UnitMap) -> Option<&UnitProperty> {
    visible_targeted_unit(units).map(|(_, unit)| unit)
}

pub(crate) fn visible_targeted_unit(units: &UnitMap) -> Option<(TargetedPlayer, &UnitProperty)> {
    units
        .get(&TargetedPlayer::Global)
        .and_then(Hidden::get_value)
        .map(|unit| (TargetedPlayer::Global, unit))
        .or_else(|| {
            units
                .iter()
                .find_map(|(target, unit)| unit.get_value().map(|unit| (*target, unit)))
        })
}

pub(crate) fn targeted_hidden<T: Copy>(values: &IndexMap<TargetedPlayer, Hidden<T>>) -> Option<T> {
    values
        .get(&TargetedPlayer::Global)
        .and_then(Hidden::get_value)
        .copied()
        .or_else(|| values.values().find_map(|value| value.get_value().copied()))
}

pub(crate) fn targeted_value<T>(values: &IndexMap<TargetedPlayer, T>) -> Option<&T> {
    values
        .get(&TargetedPlayer::Global)
        .or_else(|| values.values().next())
}
