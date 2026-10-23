//! Reuse unit orders when their global inputs and nearby units stay equal.
//!
//! The cache checks global state once per session epoch. Each entry covers
//! reached cells, adjacent cells and any stationary indirect attack range.
//! A unit change in that area invalidates the entry. Fog, hidden units and
//! uncertain health bypass the cache. Replay preserves destination checks,
//! order sequence and attack forecasts. Incomplete visits are not reused.

use super::*;
use crate::semantic::Concealment;

const BYTE_LIMIT: usize = 8 * 1024 * 1024;
const ORDER_LIMIT: usize = 4096;

#[derive(Debug, Default)]
pub(super) struct UnitOrders {
    epoch: Option<u64>,
    context: Option<State>,
    previous: Vec<Unit>,
    entries: Vec<Option<Box<Entry>>>,
    changed: Vec<u64>,
    bytes: usize,
    eviction: usize,
    enabled: bool,
}

#[derive(Debug)]
pub(super) struct Entry {
    subject: Unit,
    actions: OrderMask,
    delete: bool,
    attack_context: bool,
    valid: bool,
    coverage: Vec<u64>,
    orders: Vec<Order>,
    boundaries: Vec<usize>,
    attacks: Vec<(AttackTarget, Option<Forecast>)>,
}

fn same_context(a: &State, b: &State) -> bool {
    // Charge does not affect these actions. Power activation is not cached.
    a.ruleset == b.ruleset
        && a.settings == b.settings
        && a.board == b.board
        && a.teams == b.teams
        && a.turn == b.turn
        && a.weather == b.weather
        && a.next_unit_id == b.next_unit_id
        && a.match_state == b.match_state
        && a.players.len() == b.players.len()
        && a.players.iter().zip(b.players.iter()).all(|(a, b)| {
            a.id() == b.id()
                && a.team == b.team
                && a.funds == b.funds
                && a.status == b.status
                && a.power_state == b.power_state
                && a.commanders.len() == b.commanders.len()
                && a.commanders.iter().zip(&b.commanders).all(|(a, b)| {
                    a.id == b.id && a.active == b.active && a.power_uses == b.power_uses
                })
        })
}

impl UnitOrders {
    pub(super) fn take<V: LegalVisitor>(
        &mut self,
        session: &Session,
        unit: UnitIdx,
    ) -> Option<Box<Entry>> {
        const ALLOWED: u16 = OrderMask::WAIT
            | OrderMask::CAPTURE
            | OrderMask::SUPPLY
            | OrderMask::JOIN
            | OrderMask::LOAD
            | OrderMask::ATTACK;
        if V::ACTIONS.0 & !ALLOWED != 0 || !session.uncertain.is_empty() {
            return None;
        }
        self.prepare(session);
        if !self.enabled {
            return None;
        }
        let index = usize::from(unit.get());
        let subject = *session.state.units.at(index)?;
        let stored = self.entries[index].take();
        if let Some(entry) = &stored {
            self.bytes = self.bytes.saturating_sub(entry.bytes());
        }
        let mut entry = stored.unwrap_or_else(|| {
            Box::new(Entry {
                subject,
                actions: V::ACTIONS,
                delete: V::DELETE,
                attack_context: V::ATTACK_CONTEXT,
                valid: false,
                coverage: Vec::new(),
                orders: Vec::new(),
                boundaries: Vec::new(),
                attacks: Vec::new(),
            })
        });
        entry.valid &= entry.subject == subject
            && entry.actions == V::ACTIONS
            && entry.delete == V::DELETE
            && entry.attack_context == V::ATTACK_CONTEXT;
        entry.subject = subject;
        entry.actions = V::ACTIONS;
        entry.delete = V::DELETE;
        entry.attack_context = V::ATTACK_CONTEXT;
        Some(entry)
    }

    fn prepare(&mut self, session: &Session) {
        if self.epoch == Some(session.epoch) {
            return;
        }
        self.epoch = Some(session.epoch);
        let state = &session.state;
        self.enabled = !state.settings.fog
            && state
                .units
                .iter()
                .all(|unit| unit.concealment == Concealment::Exposed);
        if !self.enabled {
            self.entries.clear();
            self.context = None;
            self.bytes = 0;
            return;
        }
        let same = self
            .context
            .as_ref()
            .is_some_and(|prior| same_context(prior, state))
            && self.previous.len() == state.units.len()
            && self
                .previous
                .iter()
                .zip(state.units.iter())
                .all(|(a, b)| a.id == b.id);
        if same {
            let dimensions = state.board.dimensions();
            self.changed.resize(dimensions.len().div_ceil(64), 0);
            self.changed.fill(0);
            let mut reset = false;
            for (old, new) in self.previous.iter().zip(state.units.iter()) {
                let mut old = *old;
                // Check the acting unit in take. Other units can finish actions
                // without changing this unit's orders.
                old.action = new.action;
                if old == *new {
                    continue;
                }
                for unit in [&old, new] {
                    if let Location::Board { position } = unit.location {
                        let cell = dimensions.index(position).expect("a unit is on the board");
                        self.changed[cell / 64] |= 1 << (cell % 64);
                    } else {
                        reset = true;
                    }
                }
            }
            for entry in self.entries.iter_mut().flatten() {
                entry.valid &= !reset
                    && !entry
                        .coverage
                        .iter()
                        .zip(&self.changed)
                        .any(|(a, b)| a & b != 0);
            }
        } else {
            for entry in self.entries.iter().skip(state.units.len()).flatten() {
                self.bytes = self.bytes.saturating_sub(entry.bytes());
            }
            self.entries.resize_with(state.units.len(), || None);
            for entry in self.entries.iter_mut().flatten() {
                entry.valid = false;
            }
            self.context = Some(state.clone());
        }
        self.previous.clear();
        self.previous.extend(state.units.iter().copied());
    }

    pub(super) fn put(&mut self, unit: UnitIdx, entry: Box<Entry>) {
        let bytes = entry.bytes();
        if bytes > BYTE_LIMIT {
            return;
        }
        while self.bytes + bytes > BYTE_LIMIT && !self.entries.is_empty() {
            self.eviction %= self.entries.len();
            if let Some(old) = self.entries[self.eviction].take() {
                self.bytes = self.bytes.saturating_sub(old.bytes());
            }
            self.eviction += 1;
        }
        self.bytes += bytes;
        self.entries[usize::from(unit.get())] = Some(entry);
    }
}

impl Entry {
    fn bytes(&self) -> usize {
        size_of::<Self>()
            + self.coverage.capacity() * size_of::<u64>()
            + self.orders.capacity() * size_of::<Order>()
            + self.boundaries.capacity() * size_of::<usize>()
            + self.attacks.capacity() * size_of::<(AttackTarget, Option<Forecast>)>()
            + self
                .attacks
                .iter()
                .filter_map(|(_, f)| f.as_ref())
                .map(|f| f.counter_steps.capacity() * size_of::<crate::combat::CounterStep>())
                .sum::<usize>()
    }

    pub(super) fn valid(&self) -> bool {
        self.valid
    }

    pub(super) fn replay<V: LegalVisitor>(&self, state: &State, unit: UnitIdx, visitor: &mut V) {
        let mut boundary = self.boundaries.iter().copied().peekable();
        let mut attacks = self.attacks.iter();
        for index in 0..=self.orders.len() {
            while boundary.peek() == Some(&index) {
                boundary.next();
                if !visitor.continue_unit(unit) {
                    return;
                }
            }
            let Some(order) = self.orders.get(index).copied() else {
                break;
            };
            if V::ATTACK_CONTEXT && matches!(order.kind(), OrderKind::Attack(_)) {
                let (target, forecast) = attacks.next().expect("each attack has context");
                let target_unit = match target {
                    AttackTarget::Unit { unit } => state.units.get(*unit),
                    AttackTarget::Tile { .. } => None,
                };
                visitor.attack(AttackCandidate {
                    order,
                    attacker: &self.subject,
                    target: *target,
                    target_unit,
                    forecast: forecast.clone(),
                });
            } else {
                visitor.order(order);
            }
        }
    }

    pub(super) fn record<'a, V: LegalVisitor>(&'a mut self, visitor: &'a mut V) -> Recorder<'a, V> {
        self.orders.clear();
        self.boundaries.clear();
        self.attacks.clear();
        self.valid = true;
        Recorder {
            entry: self,
            visitor,
        }
    }

    pub(super) fn cover<'a, M: std::borrow::Borrow<TurnMaps<'a>>>(
        &mut self,
        state: &State,
        field: &PreparedMoveField<'a, M>,
    ) {
        let dimensions = state.board.dimensions();
        field.cover_reach(&mut self.coverage);
        let profile = ruleset::profile(self.subject.kind);
        let mut mark = |position: Pos, radius: usize| {
            let x = usize::from(position.x);
            let y = usize::from(position.y);
            for row in
                y.saturating_sub(radius)..=(y + radius).min(usize::from(dimensions.height()) - 1)
            {
                let span = radius - y.abs_diff(row);
                for col in
                    x.saturating_sub(span)..=(x + span).min(usize::from(dimensions.width()) - 1)
                {
                    let index = row * usize::from(dimensions.width()) + col;
                    self.coverage[index / 64] |= 1 << (index % 64);
                }
            }
        };
        if self.actions.has(TargetKind::Attack)
            && profile.fire_mode == ruleset::FireMode::Indirect
            && let Location::Board { position } = self.subject.location
        {
            let radius = crate::commander::effective_attack_range(
                state,
                &self.subject,
                profile
                    .indirect_range
                    .expect("an indirect weapon has a range")
                    .maximum,
                profile.domain,
                ruleset::FireMode::Indirect,
            )
            .min(u64::from(dimensions.width()) + u64::from(dimensions.height()));
            mark(position, radius as usize);
        }
    }
}

pub(super) struct Recorder<'a, V> {
    entry: &'a mut Entry,
    visitor: &'a mut V,
}

impl<V: LegalVisitor> LegalVisitor for Recorder<'_, V> {
    const ACTIONS: OrderMask = V::ACTIONS;
    const DELETE: bool = V::DELETE;
    const ATTACK_CONTEXT: bool = V::ATTACK_CONTEXT;

    fn continue_unit(&mut self, unit: UnitIdx) -> bool {
        let keep = self.visitor.continue_unit(unit);
        self.entry.valid &= keep
            && self.entry.orders.len() < ORDER_LIMIT
            && self.entry.boundaries.len() < ORDER_LIMIT;
        if self.entry.valid {
            self.entry.boundaries.push(self.entry.orders.len());
        }
        keep
    }

    fn order(&mut self, order: Order) {
        self.entry.valid &= self.entry.orders.len() < ORDER_LIMIT;
        if self.entry.valid {
            self.entry.orders.push(order);
        }
        self.visitor.order(order);
    }

    fn attack(&mut self, candidate: AttackCandidate<'_>) {
        self.entry.valid &= self.entry.orders.len() < ORDER_LIMIT;
        if self.entry.valid {
            self.entry.orders.push(candidate.order);
            self.entry
                .attacks
                .push((candidate.target, candidate.forecast.clone()));
        }
        self.visitor.attack(candidate);
    }
}
