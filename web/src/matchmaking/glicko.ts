/*
 * Glicko-2, as Mark Glickman specifies it.
 *
 * Nothing here reads the clock or the database. The caller gives the state
 * before the match, the opponents, and the deviation that time has already
 * grown. This module gives the state after the match.
 *
 * Each rated match is its own rating period. The ladder must move when a
 * match ends, and a player who waits for a period to close cannot see why
 * their rating changed. The cost is that a player who plays two matches in
 * one hour gets two updates and not one, which moves the rating a little
 * further than one combined update.
 */

/**
 * The system constant. It limits how much the volatility can change.
 *
 * Glickman suggests 0.3 to 1.2. A smaller value holds the volatility steady
 * and makes a run of surprising results move the rating less.
 */
export const GLICKO_TAU = 0.5;

/** The factor between the Glicko-1 scale players read and the Glicko-2 scale. */
const SCALE = 173.7178;

/** The rating at the centre of the scale. */
const CENTRE = 1500;

/** When the volatility search stops. */
const CONVERGENCE = 0.000001;

/** How many times the volatility search may loop before it gives up. */
const MAX_ITERATIONS = 100;

/** The Glicko-2 state for one player, on the scale the player reads. */
export interface GlickoState {
  rating: number;
  deviation: number;
  volatility: number;
}

/** One opponent in a rating period, with the score against them. */
export interface GlickoOpponent {
  rating: number;
  deviation: number;
  /** 1 for a win, 0.5 for a draw, 0 for a loss. */
  score: number;
}

/** The largest deviation the system gives out. It is an unrated player's. */
export const MAX_GLICKO_DEVIATION = 350;

/** The smallest deviation the system gives out. It stops a rating freezing. */
export const MIN_GLICKO_DEVIATION = 30;

function toGlicko2(state: GlickoState): { mu: number; phi: number } {
  return { mu: (state.rating - CENTRE) / SCALE, phi: state.deviation / SCALE };
}

/** The weight an opponent's result carries. A less certain opponent says less. */
function g(phi: number): number {
  return 1 / Math.sqrt(1 + (3 * phi * phi) / (Math.PI * Math.PI));
}

/** The expected score against one opponent. */
function expectedScore(mu: number, opponentMu: number, opponentPhi: number): number {
  return 1 / (1 + Math.exp(-g(opponentPhi) * (mu - opponentMu)));
}

/**
 * Find the new volatility with the Illinois algorithm.
 *
 * This is step 5 of Glickman's paper. It looks for the root of f between two
 * points which the function has opposite signs at.
 */
function nextVolatility(phi: number, volatility: number, variance: number, delta: number): number {
  const a = Math.log(volatility * volatility);
  const phiSquared = phi * phi;
  const deltaSquared = delta * delta;
  const tauSquared = GLICKO_TAU * GLICKO_TAU;

  const f = (x: number): number => {
    const exp = Math.exp(x);
    const denominator = phiSquared + variance + exp;
    return (
      (exp * (deltaSquared - phiSquared - variance - exp)) / (2 * denominator * denominator) -
      (x - a) / tauSquared
    );
  };

  let lower = a;
  let upper: number;
  if (deltaSquared > phiSquared + variance) {
    upper = Math.log(deltaSquared - phiSquared - variance);
  } else {
    // Walk down in steps of tau until the function turns negative.
    let step = 1;
    while (f(a - step * GLICKO_TAU) < 0 && step <= MAX_ITERATIONS) step += 1;
    upper = a - step * GLICKO_TAU;
  }

  let lowerValue = f(lower);
  let upperValue = f(upper);

  for (let round = 0; round < MAX_ITERATIONS; round += 1) {
    if (Math.abs(upper - lower) <= CONVERGENCE) break;
    const middle = lower + ((lower - upper) * lowerValue) / (upperValue - lowerValue);
    const middleValue = f(middle);
    if (middleValue * upperValue <= 0) {
      lower = upper;
      lowerValue = upperValue;
    } else {
      lowerValue /= 2;
    }
    upper = middle;
    upperValue = middleValue;
  }

  return Math.exp(lower / 2);
}

function clampDeviation(deviation: number): number {
  return Math.min(Math.max(deviation, MIN_GLICKO_DEVIATION), MAX_GLICKO_DEVIATION);
}

/**
 * The state after a rating period.
 *
 * `state.deviation` must already carry the growth for the time since the last
 * rated match. `readTimeDeviation` in `ranked_display.ts` calculates it, and
 * the rating pass applies it before it calls this function.
 *
 * With no opponents only the deviation moves, which is the idle case from the
 * paper.
 */
export function updateRating(
  state: GlickoState,
  opponents: readonly GlickoOpponent[],
): GlickoState {
  const { mu, phi } = toGlicko2(state);

  if (opponents.length === 0) {
    const phiAfter = Math.sqrt(phi * phi + state.volatility * state.volatility);
    return {
      rating: state.rating,
      deviation: clampDeviation(phiAfter * SCALE),
      volatility: state.volatility,
    };
  }

  // Step 3 and step 4: the variance of the result, and the direction it points.
  let varianceSum = 0;
  let deltaSum = 0;
  for (const opponent of opponents) {
    const { mu: opponentMu, phi: opponentPhi } = toGlicko2({
      rating: opponent.rating,
      deviation: opponent.deviation,
      volatility: state.volatility,
    });
    const weight = g(opponentPhi);
    const expected = expectedScore(mu, opponentMu, opponentPhi);
    varianceSum += weight * weight * expected * (1 - expected);
    deltaSum += weight * (opponent.score - expected);
  }

  const variance = 1 / varianceSum;
  const delta = variance * deltaSum;

  // Step 5 to step 7: the new volatility, then the new deviation and rating.
  const volatilityAfter = nextVolatility(phi, state.volatility, variance, delta);
  const phiStar = Math.sqrt(phi * phi + volatilityAfter * volatilityAfter);
  const phiAfter = 1 / Math.sqrt(1 / (phiStar * phiStar) + 1 / variance);
  const muAfter = mu + phiAfter * phiAfter * deltaSum;

  return {
    rating: muAfter * SCALE + CENTRE,
    deviation: clampDeviation(phiAfter * SCALE),
    volatility: volatilityAfter,
  };
}
