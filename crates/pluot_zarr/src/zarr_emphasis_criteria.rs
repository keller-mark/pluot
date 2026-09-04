use std::sync::Arc;
use serde::{Deserialize, Serialize};
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria, QuantitativeCriteriaParams};

use crate::zarr_numeric_data::{arr_cache_key, load_arr_as_numeric_data_memoized};

/// Filtering or selection criteria for a zarr-backed layer's data items.
///
/// Mirrors [`EmphasisCriteria`], but the per-element `codes`/`values` column
/// is referenced by a zarr array path (`codes_key`/`values_key`) rather than
/// an inlined [`NumericData`]. Resolve into an [`EmphasisCriteria`] (loading
/// the referenced array) via [`resolve_zarr_emphasis_criteria`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "criteria_mode", content = "criteria_params")]
pub enum ZarrEmphasisCriteria {
    Categorical(ZarrCategoricalCriteriaParams),
    Quantitative(ZarrQuantitativeCriteriaParams),
}

/// A categorical column in categories+codes format, referenced by zarr array
/// path, along with the set of category codes that are included.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZarrCategoricalCriteriaParams {
    /// Zarr array path to a category code per item.
    pub codes_key: String,
    /// The category codes to include. An explicit empty list means nothing is included.
    pub included_codes: Vec<i64>,
}

/// A quantitative column, referenced by zarr array path, along with the
/// included range. Omitting `min` or `max` implicitly means -infinity/+infinity
/// in that direction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZarrQuantitativeCriteriaParams {
    /// Zarr array path to a value per item.
    pub values_key: String,
    /// Inclusive lower bound of included values. Omitted implies -infinity.
    pub min: Option<f32>,
    /// Inclusive upper bound of included values. Omitted implies +infinity.
    pub max: Option<f32>,
}

impl ZarrEmphasisCriteria {
    pub(crate) fn array_path(&self) -> &str {
        match self {
            ZarrEmphasisCriteria::Categorical(params) => &params.codes_key,
            ZarrEmphasisCriteria::Quantitative(params) => &params.values_key,
        }
    }
}

/// Resolve a list of [`ZarrEmphasisCriteria`] into [`EmphasisCriteria`] by
/// loading each referenced zarr array. Each array is fetched concurrently and
/// memoized by `(store_name, array_path)` alone — see [`arr_cache_key`] — so a
/// criterion's array is never re-loaded just because its thresholds, its
/// position in the list, or the calling layer changed.
pub async fn resolve_zarr_emphasis_criteria(
    store: Arc<dyn AsyncReadableStorageTraits>,
    criteria: &[ZarrEmphasisCriteria],
    store_name: &str,
    cache_enabled: bool,
) -> Result<Vec<EmphasisCriteria>, zarrs::array::ArrayError> {
    let futures = criteria.iter().map(|c| {
        load_arr_as_numeric_data_memoized(store.clone(), store_name, c.array_path(), cache_enabled)
    });

    let resolved_data: Vec<Arc<NumericData>> = futures::future::join_all(futures)
        .await
        .into_iter()
        .collect::<Result<_, _>>()?;

    Ok(criteria
        .iter()
        .zip(resolved_data)
        .map(|(c, data)| match c {
            ZarrEmphasisCriteria::Categorical(params) => EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: data.as_ref().clone(),
                included_codes: params.included_codes.clone(),
            }),
            ZarrEmphasisCriteria::Quantitative(params) => EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: data.as_ref().clone(),
                min: params.min,
                max: params.max,
            }),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorical_serializes_to_adjacently_tagged_json() {
        let criteria = ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/n_1000000/class_labels".to_string(),
            included_codes: vec![0, 2, 4],
        });

        let value = serde_json::to_value(&criteria).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "criteria_mode": "Categorical",
                "criteria_params": {
                    "codes_key": "/n_1000000/class_labels",
                    "included_codes": [0, 2, 4],
                },
            })
        );

        let round_tripped: ZarrEmphasisCriteria = serde_json::from_value(value).unwrap();
        assert_eq!(round_tripped.array_path(), "/n_1000000/class_labels");
    }

    #[test]
    fn quantitative_serializes_to_adjacently_tagged_json() {
        let criteria = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/n_1000/x_coords".to_string(),
            min: Some(0.0),
            max: None,
        });

        let value = serde_json::to_value(&criteria).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "criteria_mode": "Quantitative",
                "criteria_params": {
                    "values_key": "/n_1000/x_coords",
                    "min": 0.0,
                    "max": null,
                },
            })
        );
    }

    #[test]
    fn criteria_arr_cache_key_ignores_thresholds() {
        // Two brush positions over the same column must resolve to one cache
        // entry, so dragging a brush never re-loads the underlying array.
        let narrow = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/obs/DISTANCE".to_string(),
            min: Some(0.0),
            max: Some(100.0),
        });
        let wide = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/obs/DISTANCE".to_string(),
            min: Some(0.0),
            max: Some(5000.0),
        });
        assert_eq!(
            arr_cache_key("store", narrow.array_path()),
            arr_cache_key("store", wide.array_path()),
        );

        // Likewise for a categorical column as its included codes change.
        let some_codes = ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/obs/CARRIER".to_string(),
            included_codes: vec![0, 2],
        });
        let other_codes = ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/obs/CARRIER".to_string(),
            included_codes: vec![],
        });
        assert_eq!(
            arr_cache_key("store", some_codes.array_path()),
            arr_cache_key("store", other_codes.array_path()),
        );
    }

    #[test]
    fn criteria_arr_cache_key_distinguishes_store_and_path() {
        assert_ne!(
            arr_cache_key("store_a", "/obs/DISTANCE"),
            arr_cache_key("store_b", "/obs/DISTANCE"),
        );
        assert_ne!(
            arr_cache_key("store", "/obs/DISTANCE"),
            arr_cache_key("store", "/obs/DEP_TIME"),
        );
    }

    #[test]
    fn array_path_selects_the_variants_key() {
        let categorical = ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "codes".to_string(),
            included_codes: vec![],
        });
        assert_eq!(categorical.array_path(), "codes");

        let quantitative = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "values".to_string(),
            min: None,
            max: None,
        });
        assert_eq!(quantitative.array_path(), "values");
    }
}
