//! Does the evaluation function know who is winning?
//!
//! [`crate::eval`] answers what a position is worth. Nothing about writing that
//! function proves it is right, and a wrong one is worse than none: a search
//! built on it plays every line toward a number that does not mean anything.
//!
//! So it is measured before anything is built on it, and it is measured without
//! a new agent. The arena already plays thousands of games it knows the result
//! of. Sampling the evaluation at every turn boundary of those games gives a
//! prediction and, when the game ends, the answer to it. Three numbers come out
//! of that:
//!
//! - **Accuracy.** How often the sign is right: does the side the function says
//!   is ahead go on to win? A function that cannot beat a coin is not one.
//! - **Brier score.** The squared error of the probability, against the 0.25 a
//!   coin scores. This is the number to watch, because it prices confidence:
//!   being sure and wrong costs more than being unsure and wrong.
//! - **The calibration table.** Of the positions the function called 70%, how
//!   many were won? A function can rank positions well and still state the
//!   odds badly, and only the table tells the two apart.
//!
//! Accuracy by day is the reading that says the most. Every function is right
//! on the last turn, when the headquarters is already surrounded. A function
//! that is right on day five is one that knows something.
//!
//! The temperature is not guessed. [`Calibration::fit_temperature`] scans for
//! the one that scores the best log loss over the samples in hand, so the
//! probability is fitted to games this crate really played rather than to an
//! idea of how much a lead is worth.
//!
//! **What a good number here does not prove.** These games are agents playing
//! themselves, so the positions are the positions those agents reach. An
//! evaluation fitted to them is fitted to that band of play, and it is worth
//! rerunning the report against every weighting on the ladder before trusting
//! it as the leaf of a search.

use std::fmt;

/// One position, and what became of the game it was in.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Sample {
    /// The day the position was on.
    pub day: u32,
    /// The seat the value was read for.
    pub seat: usize,
    /// The seat whose turn the position is the start of.
    ///
    /// Half of the samples of a duel are taken after our turn and half after
    /// theirs, and a value read in funds is not the same on both: we have
    /// just spent, and they have not. Kept so that a run written out can be
    /// read one way and then the other, because a function that is right only
    /// on its own turn is a function with a tempo term missing.
    pub active: usize,
    /// What [`crate::Evaluator::value`] answered, in funds.
    pub value: f64,
    /// What the seat scored: one for a win, nothing for a loss, a half for a
    /// draw. The same scale the arena scores a game on.
    pub label: f64,
}

/// The samples of a run, and the report over them.
///
/// Samples are pushed as a game is played and labelled when it ends, because
/// the label is not known until then. [`Calibration::finish_game`] is what
/// applies it; samples pushed and never finished are dropped by
/// [`Calibration::report`], which is what an abandoned game should contribute.
#[derive(Clone, Debug, Default)]
pub struct Calibration {
    samples: Vec<Sample>,
    /// Where the game being played started, and the end of the labelled run.
    labelled: usize,
    games: u32,
}

impl Calibration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one position of the game being played.
    pub fn sample(&mut self, day: u32, seat: usize, active: usize, value: f64) {
        self.samples.push(Sample {
            day,
            seat,
            active,
            value,
            label: f64::NAN,
        });
    }

    /// Label everything sampled since the last game ended.
    ///
    /// `label` is what the seat those samples were read for scored: one, a
    /// half, or nothing.
    pub fn finish_game(&mut self, label: f64) {
        for sample in &mut self.samples[self.labelled..] {
            sample.label = label;
        }
        self.labelled = self.samples.len();
        self.games += 1;
    }

    /// Throw away the samples of a game that has no result.
    pub fn abandon_game(&mut self) {
        self.samples.truncate(self.labelled);
    }

    /// The labelled samples, in the order they were played.
    pub fn samples(&self) -> &[Sample] {
        &self.samples[..self.labelled]
    }

    pub const fn games(&self) -> u32 {
        self.games
    }

    /// The temperature that scores the best log loss over these samples.
    ///
    /// A scan and not a solver. The curve has one parameter, the samples are
    /// cheap to score, and a scan cannot walk off a cliff or fail to converge.
    /// The grid is logarithmic because the parameter is a scale: the step from
    /// 1,000 to 2,000 funds matters as much as the step from 100,000 to
    /// 200,000.
    pub fn fit_temperature(&self) -> f64 {
        const LOW: f64 = 250.0;
        const HIGH: f64 = 1_000_000.0;
        const STEPS: u32 = 240;

        let samples = self.samples();
        if samples.is_empty() {
            return crate::EvalWeights::DEFAULT.temperature;
        }

        let ratio = (HIGH / LOW).ln() / f64::from(STEPS);
        let mut best = LOW;
        let mut best_loss = f64::INFINITY;
        for step in 0..=STEPS {
            let temperature = LOW * (ratio * f64::from(step)).exp();
            let loss = log_loss(samples, temperature);
            if loss < best_loss {
                best_loss = loss;
                best = temperature;
            }
        }
        best
    }

    /// Score the samples at one temperature.
    pub fn report(&self, temperature: f64) -> Report {
        let samples = self.samples();
        let mut accuracy = 0.0;
        let mut brier = 0.0;
        let mut days: Vec<DayRow> = Vec::new();
        let mut buckets: Vec<Bucket> = (0..BUCKETS)
            .map(|index| Bucket {
                low: f64::from(index) / f64::from(BUCKETS),
                high: f64::from(index + 1) / f64::from(BUCKETS),
                samples: 0,
                predicted: 0.0,
                observed: 0.0,
            })
            .collect();

        for sample in samples {
            let probability = crate::eval::win_probability(sample.value, temperature);
            let agreement = agreement(sample.value, sample.label);
            let error = (probability - sample.label).powi(2);
            accuracy += agreement;
            brier += error;

            let row = match days.iter().position(|row| row.day == sample.day) {
                Some(index) => &mut days[index],
                None => {
                    days.push(DayRow {
                        day: sample.day,
                        samples: 0,
                        accuracy: 0.0,
                        brier: 0.0,
                        mean_value: 0.0,
                    });
                    days.last_mut().expect("a row was just pushed")
                }
            };
            row.samples += 1;
            row.accuracy += agreement;
            row.brier += error;
            row.mean_value += sample.value;

            let index = ((probability * f64::from(BUCKETS)) as usize).min(BUCKETS as usize - 1);
            let bucket = &mut buckets[index];
            bucket.samples += 1;
            bucket.predicted += probability;
            bucket.observed += sample.label;
        }

        let count = samples.len().max(1) as f64;
        for row in &mut days {
            let row_count = row.samples.max(1) as f64;
            row.accuracy /= row_count;
            row.brier /= row_count;
            row.mean_value /= row_count;
        }
        days.sort_by_key(|row| row.day);
        for bucket in &mut buckets {
            let bucket_count = bucket.samples.max(1) as f64;
            bucket.predicted /= bucket_count;
            bucket.observed /= bucket_count;
        }
        buckets.retain(|bucket| bucket.samples > 0);

        // What a function that knows nothing scores: an even chance, every
        // time. Every number above is only worth reading against these.
        let mut baseline_brier = 0.0;
        for sample in samples {
            baseline_brier += (0.5 - sample.label).powi(2);
        }

        Report {
            games: self.games,
            samples: samples.len(),
            temperature,
            accuracy: accuracy / count,
            brier: brier / count,
            log_loss: log_loss(samples, temperature),
            baseline_brier: baseline_brier / count,
            baseline_log_loss: -(0.5_f64.ln()),
            by_day: days,
            buckets,
        }
    }
}

/// How many bars the calibration table has.
const BUCKETS: u32 = 10;

/// Whether the sign of a value agrees with the result, as a share.
///
/// A half for a draw and a half for a value of nothing, because neither of
/// those is a right answer or a wrong one.
fn agreement(value: f64, label: f64) -> f64 {
    if label == 0.5 || value == 0.0 {
        return 0.5;
    }
    if (value > 0.0) == (label > 0.5) {
        1.0
    } else {
        0.0
    }
}

/// The mean negative log likelihood of the results, at one temperature.
fn log_loss(samples: &[Sample], temperature: f64) -> f64 {
    /// A probability is never stated as a certainty, so that one wrong call
    /// cannot make the whole loss infinite.
    const FLOOR: f64 = 1.0e-6;

    if samples.is_empty() {
        return f64::INFINITY;
    }
    let mut total = 0.0;
    for sample in samples {
        let probability =
            crate::eval::win_probability(sample.value, temperature).clamp(FLOOR, 1.0 - FLOOR);
        total -= sample.label * probability.ln() + (1.0 - sample.label) * (1.0 - probability).ln();
    }
    total / samples.len() as f64
}

/// What the samples of one day said.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayRow {
    pub day: u32,
    pub samples: usize,
    /// The share of the day's positions whose sign was right.
    pub accuracy: f64,
    pub brier: f64,
    /// The mean value read on this day, in funds.
    pub mean_value: f64,
}

/// One bar of the calibration table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bucket {
    pub low: f64,
    pub high: f64,
    pub samples: usize,
    /// The mean chance the function stated in this bar.
    pub predicted: f64,
    /// The share of those positions that were won.
    pub observed: f64,
}

/// What a run of samples says about the evaluation function.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub games: u32,
    pub samples: usize,
    pub temperature: f64,
    /// The share of positions whose sign was right.
    pub accuracy: f64,
    pub brier: f64,
    pub log_loss: f64,
    /// What an even chance every time scores, which is the number to beat.
    pub baseline_brier: f64,
    pub baseline_log_loss: f64,
    pub by_day: Vec<DayRow>,
    pub buckets: Vec<Bucket>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} samples over {} games, temperature {:.0} funds",
            self.samples, self.games, self.temperature
        )?;
        writeln!(f)?;
        writeln!(f, "sign accuracy        {:.4}", self.accuracy)?;
        writeln!(
            f,
            "brier                {:.4}  (a coin scores {:.4})",
            self.brier, self.baseline_brier
        )?;
        writeln!(
            f,
            "log loss             {:.4}  (a coin scores {:.4})",
            self.log_loss, self.baseline_log_loss
        )?;

        writeln!(f)?;
        writeln!(f, "by day")?;
        writeln!(f, "  day  samples  accuracy   brier   mean value")?;
        for row in &self.by_day {
            writeln!(
                f,
                "  {:>3}  {:>7}    {:.4}  {:.4}  {:>11.0}",
                row.day, row.samples, row.accuracy, row.brier, row.mean_value
            )?;
        }

        writeln!(f)?;
        writeln!(f, "calibration")?;
        writeln!(f, "  bucket range  samples   mean predicted   won")?;
        for bucket in &self.buckets {
            writeln!(
                f,
                "  {:.1} to {:.1}  {:>9}    {:.3}  {:.3}",
                bucket.low, bucket.high, bucket.samples, bucket.predicted, bucket.observed
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run of games, alternating won and lost, whose values say so.
    fn perfect(games: u32) -> Calibration {
        let mut calibration = Calibration::new();
        for game in 0..games {
            let won = game % 2 == 0;
            for day in 1..=5 {
                calibration.sample(
                    day,
                    0,
                    usize::from(day % 2 == 0),
                    if won { 40_000.0 } else { -40_000.0 },
                );
            }
            calibration.finish_game(if won { 1.0 } else { 0.0 });
        }
        calibration
    }

    #[test]
    fn a_function_that_knows_the_result_scores_the_result() {
        let report = perfect(20).report(20_000.0);
        assert_eq!(report.samples, 100);
        assert_eq!(report.games, 20);
        assert!((report.accuracy - 1.0).abs() < 1e-9);
        assert!(report.brier < report.baseline_brier);
        assert!(report.log_loss < report.baseline_log_loss);
    }

    /// A function that says nothing scores what a coin scores, and no better.
    #[test]
    fn a_function_that_says_nothing_scores_what_a_coin_scores() {
        let mut calibration = Calibration::new();
        for game in 0..20 {
            calibration.sample(1, 0, 0, 0.0);
            calibration.finish_game(f64::from(game % 2));
        }
        let report = calibration.report(20_000.0);
        assert!((report.accuracy - 0.5).abs() < 1e-9);
        assert!((report.brier - report.baseline_brier).abs() < 1e-9);
        assert!((report.log_loss - report.baseline_log_loss).abs() < 1e-9);
    }

    /// The fit answers a temperature that scores better than the wrong ones.
    #[test]
    fn the_fitted_temperature_is_the_best_of_the_scan() {
        let calibration = perfect(20);
        let fitted = calibration.fit_temperature();
        let best = calibration.report(fitted).log_loss;
        for temperature in [500.0, 5_000.0, 50_000.0, 500_000.0] {
            assert!(
                best <= calibration.report(temperature).log_loss + 1e-12,
                "the fit at {fitted} lost to {temperature}"
            );
        }
    }

    /// A game that never ends contributes nothing.
    #[test]
    fn an_abandoned_game_is_not_a_sample() {
        let mut calibration = perfect(2);
        calibration.sample(9, 0, 1, 12_345.0);
        calibration.abandon_game();
        assert_eq!(calibration.samples().len(), 10);
        assert!(calibration.samples().iter().all(|s| s.label.is_finite()));
    }

    /// Each day is one row, and the rows are in order.
    #[test]
    fn the_days_are_one_row_each_and_in_order() {
        let report = perfect(4).report(20_000.0);
        let days: Vec<u32> = report.by_day.iter().map(|row| row.day).collect();
        assert_eq!(days, vec![1, 2, 3, 4, 5]);
        assert!(report.by_day.iter().all(|row| row.samples == 4));
    }
}
