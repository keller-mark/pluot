use std::sync::Arc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::cache::use_memo_vec_string;
use pluot_core::log;
use pluot_core::render_traits::{BooleanCriteriaParams, CategoricalCriteriaParams, EmphasisCriteria, QuantitativeCriteriaParams};

use crate::adata_io::{read_string_array, NamedStore};
use crate::adata_paths::{join_path, parent_path};
use crate::zarr_numeric_data::load_arr_as_numeric_data_memoized;

/// Filtering or selection criteria for a zarr-backed layer's data items.
///
/// Mirrors [`EmphasisCriteria`], but the per-element `codes`/`values` column
/// is referenced by a zarr array path (`codes_key`/`values_key`) rather than
/// an inlined [`NumericData`]. Resolve into an [`EmphasisCriteria`] (loading
/// the referenced array) via [`resolve_zarr_emphasis_criteria`].
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "criteria_mode", content = "criteria_params")]
pub enum ZarrEmphasisCriteria {
    Categorical(ZarrCategoricalCriteriaParams),
    Quantitative(ZarrQuantitativeCriteriaParams),
    Boolean(ZarrBooleanCriteriaParams),
}

/// A boolean (or 0/1 integer) column, referenced by zarr array path. Items
/// whose value is true are included.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ZarrBooleanCriteriaParams {
    /// Zarr array path to a boolean per item.
    pub mask_key: String,
}

/// A categorical column in categories+codes format, referenced by zarr array
/// path, along with the set of categories that are included.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ZarrCategoricalCriteriaParams {
    // TODO: update this so that the user does not need to know about the /categories and /codes suffixes

    /// Zarr array path to a category code per item.
    pub codes_key: String,
    /// Zarr array path to the label of each category code, used to look up
    /// `included_categories` given as labels. Defaults to the `categories` array
    /// alongside `codes_key`, as in AnnData's categorical encoding
    /// (e.g., `/obs/cell_type/categories` for `/obs/cell_type/codes`).
    #[serde(default)]
    #[ts(optional = nullable)]
    pub categories_key: Option<String>,
    /// The categories to include, as codes (e.g., `[0, 2]`) or as labels
    /// (e.g., `["B cell", "T cell"]`). An explicit empty list means nothing is included.
    pub included_categories: IncludedCategories,
}

/// A set of categories of a categorical column.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum IncludedCategories {
    Codes(Vec<i64>),
    Labels(Vec<String>),
}

impl ZarrCategoricalCriteriaParams {
    pub fn categories_path(&self) -> String {
        self.categories_key.clone().unwrap_or_else(|| join_path(&parent_path(&self.codes_key), "categories"))
    }
}

/// The codes of `labels` among `categories`. Unknown labels are logged and skipped.
pub fn codes_of_labels(labels: &[String], categories: &[String]) -> Vec<i64> {
    labels
        .iter()
        .filter_map(|label| {
            let code = categories.iter().position(|category| category == label);
            if code.is_none() {
                log(&format!("ZarrCategoricalCriteriaParams: category \"{label}\" not found; skipping"));
            }
            code.map(|code| code as i64)
        })
        .collect()
}

async fn resolve_included_codes(
    store: Arc<dyn AsyncReadableStorageTraits>,
    params: &ZarrCategoricalCriteriaParams,
    store_name: &str,
    cache_enabled: bool,
) -> Result<Vec<i64>, zarrs::array::ArrayError> {
    match &params.included_categories {
        IncludedCategories::Codes(codes) => Ok(codes.clone()),
        IncludedCategories::Labels(labels) => {
            let categories_path = params.categories_path();
            let keys = vec!["zarr_categories".to_string(), store_name.to_string(), categories_path.clone()];
            let categories = use_memo_vec_string(async || read_string_array(&NamedStore::new(store, store_name, cache_enabled), &categories_path).await, &keys, cache_enabled).await?;
            Ok(codes_of_labels(labels, &categories))
        }
    }
}

/// A quantitative column, referenced by zarr array path, along with the
/// included range. Omitting `min` or `max` implicitly means -infinity/+infinity
/// in that direction.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ZarrQuantitativeCriteriaParams {
    /// Zarr array path to a value per item.
    pub values_key: String,
    /// Lower bound of included values, inclusive unless `min_exclusive`.
    /// Omitted implies -infinity.
    pub min: Option<f32>,
    /// Upper bound of included values, inclusive unless `max_exclusive`.
    /// Omitted implies +infinity.
    pub max: Option<f32>,
    /// Whether the lower bound excludes `min` itself. `None` (the default)
    /// means inclusive. See [`QuantitativeCriteriaParams::min_exclusive`].
    #[serde(default)]
    #[ts(optional = nullable)]
    pub min_exclusive: Option<bool>,
    /// Whether the upper bound excludes `max` itself. `None` (the default)
    /// means inclusive. See [`QuantitativeCriteriaParams::max_exclusive`].
    #[serde(default)]
    #[ts(optional = nullable)]
    pub max_exclusive: Option<bool>,
}

impl ZarrEmphasisCriteria {
    pub fn array_path(&self) -> &str {
        match self {
            ZarrEmphasisCriteria::Categorical(params) => &params.codes_key,
            ZarrEmphasisCriteria::Quantitative(params) => &params.values_key,
            ZarrEmphasisCriteria::Boolean(params) => &params.mask_key,
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
        let store = store.clone();
        async move {
            let data = load_arr_as_numeric_data_memoized(store.clone(), store_name, c.array_path(), cache_enabled).await?;
            Ok(match c {
                ZarrEmphasisCriteria::Categorical(params) => EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: data.as_ref().clone(),
                    included_codes: resolve_included_codes(store, params, store_name, cache_enabled).await?,
                }),
                ZarrEmphasisCriteria::Quantitative(params) => EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: data.as_ref().clone(),
                    min: params.min,
                    max: params.max,
                    min_exclusive: params.min_exclusive,
                    max_exclusive: params.max_exclusive,
                }),
                ZarrEmphasisCriteria::Boolean(_) => EmphasisCriteria::Boolean(BooleanCriteriaParams {
                    mask: data.as_ref().clone(),
                }),
            })
        }
    });

    futures::future::join_all(futures).await.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zarr_numeric_data::arr_cache_key;

    #[test]
    fn categorical_serializes_to_adjacently_tagged_json() {
        let criteria = ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/n_1000000/class_labels".to_string(),
            categories_key: None,
            included_categories: IncludedCategories::Codes(vec![0, 2, 4]),
        });

        let value = serde_json::to_value(&criteria).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "criteria_mode": "Categorical",
                "criteria_params": {
                    "codes_key": "/n_1000000/class_labels",
                    "categories_key": null,
                    "included_categories": [0, 2, 4],
                },
            })
        );

        let round_tripped: ZarrEmphasisCriteria = serde_json::from_value(value).unwrap();
        assert_eq!(round_tripped.array_path(), "/n_1000000/class_labels");
    }

    #[test]
    fn categorical_included_categories_deserialize_as_codes_or_labels() {
        let criteria: ZarrEmphasisCriteria = serde_json::from_value(serde_json::json!({
            "criteria_mode": "Categorical",
            "criteria_params": { "codes_key": "/obs/cell_type/codes", "included_categories": ["B cell", "T cell"] },
        }))
        .unwrap();
        let ZarrEmphasisCriteria::Categorical(params) = &criteria else {
            panic!("expected a categorical criteria");
        };
        assert!(matches!(&params.included_categories, IncludedCategories::Labels(labels) if labels == &["B cell", "T cell"]));
        assert_eq!(params.categories_path(), "/obs/cell_type/categories");

        let criteria: ZarrEmphasisCriteria = serde_json::from_value(serde_json::json!({
            "criteria_mode": "Categorical",
            "criteria_params": { "codes_key": "/obs/cell_type/codes", "categories_key": "/uns/cell_type_labels", "included_categories": [1, 3] },
        }))
        .unwrap();
        let ZarrEmphasisCriteria::Categorical(params) = &criteria else {
            panic!("expected a categorical criteria");
        };
        assert!(matches!(&params.included_categories, IncludedCategories::Codes(codes) if codes == &[1, 3]));
        assert_eq!(params.categories_path(), "/uns/cell_type_labels");
    }

    #[test]
    fn codes_of_labels_skips_unknown_labels() {
        let categories: Vec<String> = ["B cell", "T cell", "NK"].map(String::from).to_vec();
        let labels: Vec<String> = ["NK", "Unknown", "B cell"].map(String::from).to_vec();
        assert_eq!(codes_of_labels(&labels, &categories), vec![2, 0]);
    }

    #[test]
    fn quantitative_serializes_to_adjacently_tagged_json() {
        let criteria = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/n_1000/x_coords".to_string(),
            min: Some(0.0),
            max: None,
            min_exclusive: None,
            max_exclusive: None,
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
                    "min_exclusive": null,
                    "max_exclusive": null,
                },
            })
        );
    }

    #[test]
    fn quantitative_bound_exclusivity_defaults_to_inclusive() {
        // `min_exclusive`/`max_exclusive` are `#[serde(default)]`, so JSON
        // written before they existed (and any client that only cares about
        // inclusive bounds) still deserializes, as inclusive.
        let criteria: ZarrEmphasisCriteria = serde_json::from_value(serde_json::json!({
            "criteria_mode": "Quantitative",
            "criteria_params": {
                "values_key": "/obs/DISTANCE",
                "min": 0.0,
                "max": 100.0,
            },
        }))
        .unwrap();
        let ZarrEmphasisCriteria::Quantitative(params) = &criteria else {
            panic!("expected a quantitative criteria");
        };
        assert_eq!(params.min_exclusive, None);
        assert_eq!(params.max_exclusive, None);

        // And an explicitly half-open range round-trips.
        let half_open: ZarrEmphasisCriteria = serde_json::from_value(serde_json::json!({
            "criteria_mode": "Quantitative",
            "criteria_params": {
                "values_key": "/obs/DISTANCE",
                "min": 0.0,
                "max": 100.0,
                "max_exclusive": true,
            },
        }))
        .unwrap();
        let ZarrEmphasisCriteria::Quantitative(params) = &half_open else {
            panic!("expected a quantitative criteria");
        };
        assert_eq!(params.min_exclusive, None);
        assert_eq!(params.max_exclusive, Some(true));
    }

    #[test]
    fn criteria_arr_cache_key_ignores_thresholds() {
        // Two brush positions over the same column must resolve to one cache
        // entry, so dragging a brush never re-loads the underlying array.
        let narrow = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/obs/DISTANCE".to_string(),
            min: Some(0.0),
            max: Some(100.0),
            min_exclusive: None,
            max_exclusive: None,
        });
        let wide = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/obs/DISTANCE".to_string(),
            min: Some(0.0),
            max: Some(5000.0),
            min_exclusive: None,
            max_exclusive: None,
        });
        assert_eq!(
            arr_cache_key("store", narrow.array_path()),
            arr_cache_key("store", wide.array_path()),
        );

        // Likewise for a categorical column as its included codes change.
        let some_codes = ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/obs/CARRIER".to_string(),
            categories_key: None,
            included_categories: IncludedCategories::Codes(vec![0, 2]),
        });
        let other_codes = ZarrEmphasisCriteria::Categorical(ZarrCategoricalCriteriaParams {
            codes_key: "/obs/CARRIER".to_string(),
            categories_key: None,
            included_categories: IncludedCategories::Codes(vec![]),
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
            categories_key: None,
            included_categories: IncludedCategories::Codes(vec![]),
        });
        assert_eq!(categorical.array_path(), "codes");

        let quantitative = ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "values".to_string(),
            min: None,
            max: None,
            min_exclusive: None,
            max_exclusive: None,
        });
        assert_eq!(quantitative.array_path(), "values");
    }
}
