//! plato-tile-batch — Batch Tile Processing
//!
//! Import, export, transform, and validate tiles in bulk.
//! Designed for Oracle1's 2,000+ tile exports and JC1's living knowledge dumps.
//!
//! ## Why
//! Single-tile operations are for interactive use. The fleet needs to process
//! thousands of tiles at once — from exports, from training runs, from audits.

use std::collections::HashMap;

/// A minimal tile for batch processing.
#[derive(Debug, Clone)]
pub struct Tile {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub domain: String,
    pub confidence: f64,
    pub tags: Vec<String>,
}

/// Result of processing a single tile in a batch.
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub tile_id: String,
    pub status: TileStatus,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TileStatus {
    Accepted,
    Rejected,
    AcceptedWithWarnings,
    Skipped,
}

/// Result of an entire batch operation.
#[derive(Debug, Clone)]
pub struct BatchSummary {
    pub total: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub skipped: usize,
    pub warnings: usize,
    pub domains: HashMap<String, usize>,
    pub errors: Vec<String>,
}

impl BatchSummary {
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 { return 0.0; }
        (self.accepted + self.accepted_with_warnings()) as f64 / self.total as f64
    }
    pub fn accepted_with_warnings(&self) -> usize { self.warnings }
}

/// Validation rules for batch processing.
pub struct BatchValidator {
    min_confidence: f64,
    min_question_len: usize,
    min_answer_len: usize,
    max_question_len: usize,
    max_answer_len: usize,
}

impl Default for BatchValidator {
    fn default() -> Self {
        BatchValidator {
            min_confidence: 0.3,
            min_question_len: 10,
            min_answer_len: 10,
            max_question_len: 50000,
            max_answer_len: 100000,
        }
    }
}

impl BatchValidator {
    pub fn validate(&self, tile: &Tile) -> BatchResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if tile.confidence < self.min_confidence {
            errors.push(format!("confidence {:.2} < {:.2}", tile.confidence, self.min_confidence));
        }
        if tile.question.len() < self.min_question_len {
            errors.push(format!("question {} chars < {} min", tile.question.len(), self.min_question_len));
        }
        if tile.answer.len() < self.min_answer_len {
            errors.push(format!("answer {} chars < {} min", tile.answer.len(), self.min_answer_len));
        }
        if tile.question.len() > self.max_question_len {
            warnings.push(format!("question {} chars > {} max (truncatable)", tile.question.len(), self.max_question_len));
        }
        if tile.answer.len() > self.max_answer_len {
            warnings.push(format!("answer {} chars > {} max (truncatable)", tile.answer.len(), self.max_answer_len));
        }
        if tile.domain.is_empty() {
            errors.push("empty domain".into());
        }
        if tile.tags.is_empty() {
            warnings.push("no tags".into());
        }

        let status = if !errors.is_empty() { TileStatus::Rejected }
        else if !warnings.is_empty() { TileStatus::AcceptedWithWarnings }
        else { TileStatus::Accepted };

        BatchResult { tile_id: tile.id.clone(), status, errors, warnings }
    }
}

/// Batch processor.
pub struct TileBatch;

impl TileBatch {
    /// Process a batch of tiles through validation.
    pub fn validate_batch(tiles: &[Tile], validator: &BatchValidator) -> (Vec<BatchResult>, BatchSummary) {
        let mut results = Vec::new();
        let mut domains: HashMap<String, usize> = HashMap::new();
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        let mut skipped = 0usize;
        let mut warnings = 0usize;
        let mut all_errors = Vec::new();

        for tile in tiles {
            *domains.entry(tile.domain.clone()).or_insert(0) += 1;
            let result = validator.validate(tile);
            match result.status {
                TileStatus::Accepted => accepted += 1,
                TileStatus::AcceptedWithWarnings => { accepted += 1; warnings += 1; }
                TileStatus::Rejected => rejected += 1,
                TileStatus::Skipped => skipped += 1,
            }
            if result.status == TileStatus::Rejected {
                all_errors.push(format!("{}: {}", tile.id, result.errors.join("; ")));
            }
            results.push(result);
        }

        let summary = BatchSummary {
            total: tiles.len(), accepted, rejected, skipped, warnings, domains, errors: all_errors,
        };

        (results, summary)
    }

    /// Filter tiles by domain.
    pub fn filter_by_domain<'a>(tiles: &'a [Tile], domain: &str) -> Vec<&'a Tile> {
        tiles.iter().filter(|t| t.domain == domain).collect()
    }

    /// Partition tiles into accepted and rejected.
    pub fn partition<'a>(results: &[BatchResult], tiles: &'a [Tile]) -> (Vec<&'a Tile>, Vec<&'a Tile>) {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        for (r, t) in results.iter().zip(tiles.iter()) {
            match r.status {
                TileStatus::Accepted | TileStatus::AcceptedWithWarnings => accepted.push(t),
                TileStatus::Rejected | TileStatus::Skipped => rejected.push(t),
            }
        }
        (accepted, rejected)
    }

    /// Deduplicate tiles by question+answer exact match.
    pub fn dedup(tiles: &[Tile]) -> Vec<&Tile> {
        let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        tiles.iter().filter(|t| {
            let key = (t.question.clone(), t.answer.clone());
            seen.insert(key)
        }).collect()
    }

    /// Assign sequential IDs to tiles that have empty IDs.
    pub fn assign_ids(tiles: &mut [Tile], prefix: &str) {
        for (i, tile) in tiles.iter_mut().enumerate() {
            if tile.id.is_empty() {
                tile.id = format!("{}-{}", prefix, i + 1);
            }
        }
    }

    /// Compute batch statistics without full validation.
    pub fn quick_stats(tiles: &[Tile]) -> BatchStats {
        if tiles.is_empty() {
            return BatchStats::default();
        }
        let total = tiles.len();
        let avg_confidence: f64 = tiles.iter().map(|t| t.confidence).sum::<f64>() / total as f64;
        let min_conf = tiles.iter().map(|t| t.confidence).fold(f64::MAX, f64::min);
        let max_conf = tiles.iter().map(|t| t.confidence).fold(f64::MIN, f64::max);
        let avg_q_len: f64 = tiles.iter().map(|t| t.question.len() as f64).sum::<f64>() / total as f64;
        let avg_a_len: f64 = tiles.iter().map(|t| t.answer.len() as f64).sum::<f64>() / total as f64;
        let mut domains: HashMap<String, usize> = HashMap::new();
        for t in tiles { *domains.entry(t.domain.clone()).or_insert(0) += 1; }
        BatchStats { total, avg_confidence, min_confidence: min_conf, max_confidence: max_conf, avg_question_len: avg_q_len, avg_answer_len: avg_a_len, domain_count: domains.len(), domains }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    pub total: usize,
    pub avg_confidence: f64,
    pub min_confidence: f64,
    pub max_confidence: f64,
    pub avg_question_len: f64,
    pub avg_answer_len: f64,
    pub domain_count: usize,
    pub domains: HashMap<String, usize>,
}

fn make_tile(id: &str, q: &str, a: &str, domain: &str, conf: f64, tags: Vec<&str>) -> Tile {
    Tile { id: id.into(), question: q.into(), answer: a.into(), domain: domain.into(), confidence: conf, tags: tags.iter().map(|s| s.to_string()).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_good_tile() {
        let v = BatchValidator::default();
        let t = make_tile("t1", "What is PLATO?", "Training pipeline for agents.", "plato", 0.9, vec!["training", "pipeline"]);
        let r = v.validate(&t);
        assert_eq!(r.status, TileStatus::Accepted);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn test_validate_low_confidence() {
        let v = BatchValidator::default();
        let t = make_tile("t2", "What is PLATO?", "Training pipeline.", "plato", 0.1, vec![]);
        let r = v.validate(&t);
        assert_eq!(r.status, TileStatus::Rejected);
        assert!(r.errors.iter().any(|e| e.contains("confidence")));
    }

    #[test]
    fn test_validate_short_content() {
        let v = BatchValidator::default();
        let t = make_tile("t3", "Short", "Short", "x", 0.9, vec![]);
        let r = v.validate(&t);
        assert_eq!(r.status, TileStatus::Rejected);
    }

    #[test]
    fn test_validate_empty_domain() {
        let v = BatchValidator::default();
        let t = make_tile("t4", "What is flux?", "Bytecode runtime for agents.", "", 0.9, vec![]);
        let r = v.validate(&t);
        assert!(r.errors.iter().any(|e| e.contains("domain")));
    }

    #[test]
    fn test_validate_warnings() {
        let v = BatchValidator::default();
        let t = make_tile("t5", "What is constraint theory?", "Geometric snapping for deterministic computation across all machines.", "ct", 0.9, vec![]);
        let r = v.validate(&t);
        assert_eq!(r.status, TileStatus::AcceptedWithWarnings);
        assert!(r.warnings.iter().any(|w| w.contains("tags")));
    }

    #[test]
    fn test_batch_validate_100() {
        let tiles: Vec<Tile> = (0..100).map(|i| {
            if i < 90 { make_tile(&format!("g{}", i), &format!("Question number {} about PLATO and tiles", i), &format!("Answer number {} describing the training pipeline", i), "plato", 0.9, vec!["test"]) }
            else { make_tile(&format!("b{}", i), "Short", "Short", "x", 0.1, vec![]) }
        }).collect();
        let (results, summary) = TileBatch::validate_batch(&tiles, &BatchValidator::default());
        assert_eq!(summary.total, 100);
        assert_eq!(summary.accepted, 90);
        assert_eq!(summary.rejected, 10);
        assert!(summary.pass_rate() > 0.89);
        assert_eq!(summary.domains.get("plato").copied().unwrap_or(0), 90);
    }

    #[test]
    fn test_filter_by_domain() {
        let tiles = vec![
            make_tile("t1", "Q1 about PLATO?", "A1 about PLATO.", "plato", 0.9, vec![]),
            make_tile("t2", "Q2 about flux?", "A2 about flux.", "flux", 0.9, vec![]),
            make_tile("t3", "Q3 about PLATO?", "A3 about PLATO.", "plato", 0.8, vec![]),
        ];
        let plato = TileBatch::filter_by_domain(&tiles, "plato");
        assert_eq!(plato.len(), 2);
    }

    #[test]
    fn test_partition() {
        let tiles = vec![
            make_tile("g1", "Good question about tiles", "Good answer about tiles", "plato", 0.9, vec!["test"]),
            make_tile("b1", "Short", "Short", "x", 0.1, vec![]),
        ];
        let (results, _) = TileBatch::validate_batch(&tiles, &BatchValidator::default());
        let (accepted, rejected) = TileBatch::partition(&results, &tiles);
        assert_eq!(accepted.len(), 1);
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn test_dedup() {
        let tiles = vec![
            make_tile("t1", "Q?", "A.", "x", 0.9, vec![]),
            make_tile("t2", "Q?", "A.", "x", 0.9, vec![]),
            make_tile("t3", "Different Q", "Different A", "x", 0.9, vec![]),
        ];
        let deduped = TileBatch::dedup(&tiles);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_assign_ids() {
        let mut tiles = vec![
            Tile { id: String::new(), question: "Q".into(), answer: "A".into(), domain: "x".into(), confidence: 0.9, tags: vec![] },
            Tile { id: "existing".into(), question: "Q".into(), answer: "A".into(), domain: "x".into(), confidence: 0.9, tags: vec![] },
        ];
        TileBatch::assign_ids(&mut tiles, "batch");
        assert_eq!(tiles[0].id, "batch-1");
        assert_eq!(tiles[1].id, "existing");
    }

    #[test]
    fn test_quick_stats() {
        let tiles = vec![
            make_tile("t1", "Q1 about PLATO?", "A1 about PLATO.", "plato", 0.9, vec![]),
            make_tile("t2", "Q2 about flux?", "A2 about flux.", "flux", 0.8, vec![]),
            make_tile("t3", "Q3 about tiles?", "A3 about tiles.", "plato", 0.7, vec![]),
        ];
        let stats = TileBatch::quick_stats(&tiles);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.domain_count, 2);
        assert!((stats.avg_confidence - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_quick_stats_empty() {
        let stats = TileBatch::quick_stats(&[]);
        assert_eq!(stats.total, 0);
    }

    #[test]
    fn test_summary_errors_list() {
        let tiles = vec![
            make_tile("bad1", "Q?", "A.", "", 0.1, vec![]),
            make_tile("bad2", "Tiny", "Tiny", "x", 0.1, vec![]),
        ];
        let (_, summary) = TileBatch::validate_batch(&tiles, &BatchValidator::default());
        assert_eq!(summary.rejected, 2);
        assert_eq!(summary.errors.len(), 2);
    }
}
