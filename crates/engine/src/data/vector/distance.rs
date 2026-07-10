//! Exact vector scoring.

use crate::diagnostics::{EngineError, EngineResult};

use super::{VectorDistanceMetric, VectorEmbedding};

pub(crate) fn vector_score(
    query: &VectorEmbedding,
    candidate: &VectorEmbedding,
    metric: VectorDistanceMetric,
) -> EngineResult<f32> {
    if query.dimension() != candidate.dimension() {
        return Err(EngineError::invalid_input(
            "invalid_argument.engine.vector_dimension",
            "vector score dimension mismatch",
        ));
    }
    let score = match metric {
        VectorDistanceMetric::Cosine => cosine_similarity(query.as_slice(), candidate.as_slice()),
        VectorDistanceMetric::Euclidean => {
            euclidean_similarity(query.as_slice(), candidate.as_slice())
        }
        VectorDistanceMetric::DotProduct => dot_product(query.as_slice(), candidate.as_slice()),
    };
    Ok(score)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for (left_value, right_value) in left.iter().zip(right) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    let denominator = (left_norm * right_norm).sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}

fn euclidean_similarity(left: &[f32], right: &[f32]) -> f32 {
    let distance = left
        .iter()
        .zip(right)
        .map(|(left_value, right_value)| {
            let delta = left_value - right_value;
            delta * delta
        })
        .sum::<f32>()
        .sqrt();
    1.0 / (1.0 + distance)
}

fn dot_product(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left_value, right_value)| left_value * right_value)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::vector_score;
    use crate::data::vector::{VectorDistanceMetric, VectorEmbedding};

    #[test]
    fn metric_scores_are_higher_is_better() {
        let query = VectorEmbedding::new([1.0, 0.0]).expect("valid query");
        let same = VectorEmbedding::new([1.0, 0.0]).expect("valid vector");
        let orthogonal = VectorEmbedding::new([0.0, 1.0]).expect("valid vector");
        assert!(
            vector_score(&query, &same, VectorDistanceMetric::Cosine).expect("score")
                > vector_score(&query, &orthogonal, VectorDistanceMetric::Cosine).expect("score")
        );
        let euclidean =
            vector_score(&query, &same, VectorDistanceMetric::Euclidean).expect("score");
        assert!((euclidean - 1.0).abs() < f32::EPSILON);
        let dot = vector_score(&query, &same, VectorDistanceMetric::DotProduct).expect("score");
        assert!((dot - 1.0).abs() < f32::EPSILON);
    }
}
