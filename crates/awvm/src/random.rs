//! The explicit random tape a caller supplies alongside a command.
//!
//! AWVM never rolls dice. Every non-deterministic decision is supplied as a
//! token, in the order the reducer draws them, and the reducer reports how many
//! it consumed so a replay can be checked token-for-token
//! (`spec/schema/random.schema.json`).
//!
//! [`RandomTape`] is that cursor. Counting draws is its job rather than the
//! reducer's, because the count and the draws must agree, and a hand-written
//! total agrees only until someone adds a branch.

use serde::{Deserialize, Serialize};

use crate::commander::Domain;
use crate::semantic::WeatherKind;

/// One supplied random draw.
///
/// The tag and payload are separate keys on the wire — `{"type": …, "value": …}`
/// — which is serde's adjacent tagging.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum RandomToken {
    CombatGoodLuck(i64),
    CombatBadLuck(i64),
    WeatherSelection(WeatherKind),
}

/// Which end of a luck roll a draw supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Luck {
    Good,
    Bad,
}

/// The stable tag identifying a kind of supplied random token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandomTokenKind {
    CombatGoodLuck,
    CombatBadLuck,
    WeatherSelection,
}

impl RandomToken {
    pub const fn kind(self) -> RandomTokenKind {
        match self {
            Self::CombatGoodLuck(_) => RandomTokenKind::CombatGoodLuck,
            Self::CombatBadLuck(_) => RandomTokenKind::CombatBadLuck,
            Self::WeatherSelection(_) => RandomTokenKind::WeatherSelection,
        }
    }
}

/// Why a draw could not be satisfied.
///
/// All three are the specification's single "missing, wrong-type, or
/// out-of-domain random input" execution failure; they are kept apart only so
/// the reported message says which one happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RandomError {
    /// The tape ran out.
    #[error("the random tape is missing a token")]
    Missing { expected: RandomTokenKind },
    /// The next token is not the kind the reducer asked for.
    #[error("the random tape supplied the wrong kind of token")]
    Unexpected {
        expected: RandomTokenKind,
        actual: RandomTokenKind,
    },
    /// The token is well-formed but its value is outside the range the ruleset
    /// permits for this draw.
    #[error("a random token is outside the ruleset's domain")]
    OutOfDomain {
        kind: RandomTokenKind,
        value: i64,
        minimum: i64,
        maximum: i64,
    },
}

/// Where the reducer's non-deterministic values come from.
///
/// Both methods are asked at the moment the reducer needs the value and knows
/// what would be legal, which is the whole point: `domain` is the acting
/// commander's luck range, already resolved through the commander algebra, so
/// an implementation rolling its own dice does not have to derive it. Returning
/// a value outside `domain` is the implementation's error to make, and the
/// reducer reports it exactly as it reports a malformed tape.
///
/// Implemented by [`RandomTape`] for recorded outcomes and by [`Recording`] for
/// anything else that wants its draws kept.
pub trait Entropy {
    /// A combat luck roll of the requested polarity, inside `domain`.
    fn luck(&mut self, polarity: Luck, domain: Domain) -> Result<i64, RandomError>;

    /// The weather a random-weather match turns to at a turn boundary.
    fn weather(&mut self) -> Result<WeatherKind, RandomError>;
}

impl<E: Entropy + ?Sized> Entropy for &mut E {
    fn luck(&mut self, polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        (**self).luck(polarity, domain)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        (**self).weather()
    }
}

/// An [`Entropy`] that keeps every value it produced, in draw order.
///
/// A live authority needs both halves: an RNG to play the game with, and the
/// tape that RNG produced to persist, so the same command can be replayed
/// through [`crate::transition::execute`] and checked token-for-token. Wrapping
/// its source in this is how it gets the second without giving up the first.
#[derive(Clone)]
pub struct Recording<E> {
    source: E,
    drawn: Vec<RandomToken>,
}

impl<E> Recording<E> {
    pub const fn new(source: E) -> Self {
        Self {
            source,
            drawn: Vec::new(),
        }
    }

    /// The tape produced so far, in the order the reducer drew it.
    pub fn tokens(&self) -> &[RandomToken] {
        &self.drawn
    }

    /// Take the tape, giving the source back.
    pub fn into_parts(self) -> (E, Vec<RandomToken>) {
        (self.source, self.drawn)
    }
}

impl<E: Entropy> Entropy for Recording<E> {
    fn luck(&mut self, polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        let value = self.source.luck(polarity, domain)?;
        self.drawn.push(match polarity {
            Luck::Good => RandomToken::CombatGoodLuck(value),
            Luck::Bad => RandomToken::CombatBadLuck(value),
        });
        Ok(value)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        let kind = self.source.weather()?;
        self.drawn.push(RandomToken::WeatherSelection(kind));
        Ok(kind)
    }
}

/// An in-order cursor over the tape.
///
/// The protocol boundary decodes every supplied value into a [`RandomToken`].
/// Drawing remains demand-driven: unused trailing tokens are valid and do not
/// contribute to `random_consumed`.
pub struct RandomTape<'a> {
    tokens: &'a [RandomToken],
    cursor: usize,
}

impl<'a> RandomTape<'a> {
    pub const fn new(tokens: &'a [RandomToken]) -> Self {
        Self { tokens, cursor: 0 }
    }

    /// How many tokens have been drawn. This is what `execute` reports, so it
    /// is a fact about the run rather than a number kept in step by hand.
    pub const fn consumed(&self) -> usize {
        self.cursor
    }

    fn next_token(&mut self, expected: RandomTokenKind) -> Result<RandomToken, RandomError> {
        let token = *self
            .tokens
            .get(self.cursor)
            .ok_or(RandomError::Missing { expected })?;
        self.cursor += 1;
        Ok(token)
    }

    /// Draw a luck roll, requiring it to fall inside `domain`.
    fn draw_luck(&mut self, polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        let expected = match polarity {
            Luck::Good => RandomTokenKind::CombatGoodLuck,
            Luck::Bad => RandomTokenKind::CombatBadLuck,
        };
        let token = self.next_token(expected)?;
        let value = match (token, polarity) {
            (RandomToken::CombatGoodLuck(value), Luck::Good)
            | (RandomToken::CombatBadLuck(value), Luck::Bad) => value,
            _ => {
                return Err(RandomError::Unexpected {
                    expected,
                    actual: token.kind(),
                });
            }
        };
        if !(domain.minimum..=domain.maximum).contains(&value) {
            return Err(RandomError::OutOfDomain {
                kind: expected,
                value,
                minimum: domain.minimum,
                maximum: domain.maximum,
            });
        }
        Ok(value)
    }

    /// Draw the weather the caller selected.
    fn draw_weather(&mut self) -> Result<WeatherKind, RandomError> {
        let expected = RandomTokenKind::WeatherSelection;
        let token = self.next_token(expected)?;
        match token {
            RandomToken::WeatherSelection(kind) => Ok(kind),
            _ => Err(RandomError::Unexpected {
                expected,
                actual: token.kind(),
            }),
        }
    }
}

impl Entropy for RandomTape<'_> {
    fn luck(&mut self, polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        self.draw_luck(polarity, domain)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        self.draw_weather()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::Domain;
    use serde_json::json;

    const ZERO_TO_NINE: Domain = Domain {
        minimum: 0,
        maximum: 9,
    };

    #[test]
    fn tokens_decode_from_their_adjacent_tagging() {
        assert_eq!(
            serde_json::from_value::<RandomToken>(json!({"type":"combat-good-luck","value":3}))
                .unwrap(),
            RandomToken::CombatGoodLuck(3)
        );
        assert_eq!(
            serde_json::from_value::<RandomToken>(
                json!({"type":"weather-selection","value":"rain"})
            )
            .unwrap(),
            RandomToken::WeatherSelection(WeatherKind::Rain)
        );
        assert!(
            serde_json::from_value::<RandomToken>(
                json!({"type":"weather-selection","value":"sandstorm"})
            )
            .is_err()
        );
    }

    /// The count is the point of the type: it must follow the draws, including
    /// when a draw fails partway through a command.
    #[test]
    fn the_cursor_counts_only_successful_draws() {
        let tokens = vec![
            RandomToken::CombatGoodLuck(4),
            RandomToken::CombatBadLuck(1),
        ];
        let mut tape = RandomTape::new(&tokens);
        assert_eq!(tape.consumed(), 0);
        assert_eq!(tape.luck(Luck::Good, ZERO_TO_NINE), Ok(4));
        assert_eq!(tape.consumed(), 1);
        assert_eq!(tape.luck(Luck::Bad, ZERO_TO_NINE), Ok(1));
        assert_eq!(tape.consumed(), 2);
        assert_eq!(
            tape.luck(Luck::Good, ZERO_TO_NINE),
            Err(RandomError::Missing {
                expected: RandomTokenKind::CombatGoodLuck
            })
        );
        assert_eq!(tape.consumed(), 2);
    }

    /// A decodable token advances the cursor even when it is the wrong kind.
    /// That only matters for the count, and the count never escapes a failed
    /// command: `execute` reports nothing on an error path.
    #[test]
    fn a_token_of_the_wrong_kind_still_advances_the_cursor() {
        let tokens = vec![RandomToken::CombatGoodLuck(0)];
        let mut tape = RandomTape::new(&tokens);
        assert_eq!(
            tape.weather(),
            Err(RandomError::Unexpected {
                expected: RandomTokenKind::WeatherSelection,
                actual: RandomTokenKind::CombatGoodLuck,
            })
        );
        assert_eq!(tape.consumed(), 1);
    }

    #[test]
    fn a_value_outside_the_ruleset_is_rejected() {
        let tokens = vec![
            RandomToken::CombatGoodLuck(10),
            RandomToken::WeatherSelection(WeatherKind::Rain),
        ];
        let mut tape = RandomTape::new(&tokens);
        assert_eq!(
            tape.luck(Luck::Good, ZERO_TO_NINE),
            Err(RandomError::OutOfDomain {
                kind: RandomTokenKind::CombatGoodLuck,
                value: 10,
                minimum: 0,
                maximum: 9,
            })
        );
        assert_eq!(tape.weather(), Ok(WeatherKind::Rain));
    }
}
