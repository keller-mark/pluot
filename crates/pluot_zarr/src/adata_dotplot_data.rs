// Reusable "hook" functions (in the React sense) for loading subsets of data from AnnData objects,
// and in some cases, filtering or summarizing or otherwise transforming the data as needed for visualizations.
//
// The AnnData object representation should be reusable across not just the dot plot, but also other plot types:
// - stratified dot plots (dot per cell type, gene, and sample set - and potentially further categorization along the obs axis)
// - violin plots (violin per cell type and gene)
// - stratified violin plots (violin plot per cell type, gene, and sample set - and potentially further categorization along the obs axis)
// - histograms (distribution of transcript counts for one or more genes)
// - heatmaps (subsets of the expression matrix)
// - cell segmentations (one or more gene columns of the expression matrix, across all cells or a subset of cells)
//
// For the utmost flexibility, we should support arbitrary levels of hierarchy of the obs stratification,
// and arbitrary predicate functions for filtering and selection.
//
// We always want to support both filtering and selection.
// - Filtering means that the values that are filtered out are not considered at all (no contribution to the visual representation whatsoever).
// - Selection means that we visually emphasize the selected values, and we also visually display the non-selected values (but de-emphasized or grayed-out), so we must consider both the selected and non-selected values when performing data processing and computing data distributions (we will want to compute both "foreground" and "background" distributions, so that we can display the foreground summary as emphasized, while we display the background summary as grayed-out).
//
// The return values should be semantically meaningful, with easy-to-understand struct representations (that are also efficient).
//
// The hooks below are layered accordingly, from the most generic to the most plot-specific:
//
// 1. Single-element reads, each memoized on its own so that changing one part of a plot's
//    configuration does not invalidate the rest: [`use_var_index`], [`use_obs_index`],
//    [`use_obs_categorical`], [`use_obs_numeric`], [`use_expression_column`].
// 2. [`use_obs_columns`], which loads a whole set of per-observation columns
//    ([`ObsColumnRef`]s) at once, deduplicated and concurrently. A "column" here is
//    anything indexed by observation: an `obs` column, or one gene's column of an
//    expression matrix (so predicates can be written against gene expression, not just
//    metadata).
// 3. [`use_obs_grouping`], which turns an arbitrarily deep stratification hierarchy
//    ([`ObsStratifyLevel`]s) plus an optional filter and selection [`ObsPredicate`] into
//    an [`ObsGrouping`]: the axis categories of every level, and, per leaf group, the
//    foreground (selected) and background (unselected) observation indices.
// 4. Per-plot summarization of those groups. Only the dot plot lives here so far
//    ([`use_dotplot_data`], built on [`summarize_expression`]). A violin/histogram hook
//    would reuse steps 1-3 unchanged and swap [`summarize_expression`] for a quantile or
//    binning step; a heatmap or cell-segmentation hook would use the loaded columns
//    directly and skip grouping altogether.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::cache::{use_memo_numeric_data, use_memo_vec_f32, use_memo_vec_string};
use pluot_core::log;
use pluot_core::maybe::{MaybeSend, MaybeSync};
use pluot_core::numeric_data::NumericData;
use zarrs::array::ArrayError;
use zarrs::storage::AsyncReadableStorageTraits;

use crate::adata_io::{read_dataframe_index, read_dense_column_f32, read_string_array};
use crate::zarr_numeric_data::load_arr_as_numeric_data;

/// The store of a single AnnData-Zarr object (i.e. rooted at the `anndata`-encoded group).
pub type AdataStore = Arc<dyn AsyncReadableStorageTraits>;

// ---------------------------------------------------------------------------
// Memoization
// ---------------------------------------------------------------------------

/// Defines a `use_memo`-style function backed by its own thread-local cache, for
/// one concrete return type. Mirrors the hand-written `use_memo_*` functions in
/// `pluot_core::cache` (see the TODO there about generalizing them); these live
/// here because the types they cache are AnnData-specific.
///
/// The cache is borrowed only for the duration of a single lookup or insert, and
/// three things are deliberately kept *outside* those borrows, since each of them can
/// run arbitrary code that might reach this same cache again (these hooks nest: a
/// memoized value's initializer awaits other memoized values):
/// - the initializer, which is awaited with no borrow held at all;
/// - the destructor of any entry an insert displaces, which is dropped afterwards;
/// - a failed borrow, which degrades to a cache miss (recompute, or skip the insert)
///   rather than panicking.
macro_rules! define_use_memo {
    ($fn_name:ident, $cache:ident, $ty:ty) => {
        thread_local! {
            static $cache: RefCell<Option<HashMap<Vec<String>, Arc<$ty>>>> = const { RefCell::new(None) };
        }

        async fn $fn_name<E>(
            initializer: impl AsyncFnOnce() -> Result<$ty, E>,
            keys: &[String],
            cache_enabled: bool,
        ) -> Result<Arc<$ty>, E> {
            if !cache_enabled {
                return Ok(Arc::new(initializer().await?));
            }

            let cached = $cache
                .try_with(|cache| {
                    cache.try_borrow().ok().and_then(|map| map.as_ref().and_then(|m| m.get(keys).cloned()))
                })
                .ok()
                .flatten();
            if let Some(value) = cached {
                return Ok(value);
            }

            let value = Arc::new(initializer().await?);

            // `insert` hands back whatever entry it replaced; hold onto it so that it is
            // dropped below, once the borrow has been released.
            let displaced = $cache
                .try_with(|cache| match cache.try_borrow_mut() {
                    Ok(mut map) => map.get_or_insert_with(HashMap::new).insert(keys.to_vec(), value.clone()),
                    Err(_) => None,
                })
                .ok()
                .flatten();
            drop(displaced);

            Ok(value)
        }
    };
}

define_use_memo!(use_memo_obs_grouping, USE_MEMO_CACHE_OBS_GROUPING, ObsGrouping);
define_use_memo!(use_memo_dotplot_data, USE_MEMO_CACHE_DOTPLOT_DATA, DotPlotData);

// ---------------------------------------------------------------------------
// Per-observation columns
// ---------------------------------------------------------------------------

/// A reference to one column of per-observation values, i.e. one value per row of
/// `obs`. Used both to stratify the obs axis and as the input of a predicate.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ObsColumnRef {
    /// A `categorical`-encoded column of `obs`, by column name (e.g. `"cell_type"`,
    /// stored at `obs/cell_type/{categories,codes}`).
    Categorical(String),
    /// A numeric (`array`-encoded) column of `obs`, by column name (e.g. `"n_counts"`).
    Numeric(String),
    /// One gene's column of an expression matrix: `X` when `layer` is `None`, or
    /// `layers/<layer>` otherwise. The gene is named as it appears in `var.index`.
    Expression { layer: Option<String>, var_name: String },
}

impl ObsColumnRef {
    /// Convenience constructor for a gene's column of `X`.
    pub fn expression(var_name: impl Into<String>) -> Self {
        ObsColumnRef::Expression { layer: None, var_name: var_name.into() }
    }

    /// Stable, human-readable identifier, used both in cache keys and in log messages.
    pub fn cache_key(&self) -> String {
        match self {
            ObsColumnRef::Categorical(column) => format!("obs_categorical:{column}"),
            ObsColumnRef::Numeric(column) => format!("obs_numeric:{column}"),
            ObsColumnRef::Expression { layer, var_name } => {
                format!("expression:{}:{var_name}", layer.as_deref().unwrap_or("X"))
            }
        }
    }
}

/// The loaded values of one [`ObsColumnRef`], in obs (row) order. Cloning is cheap:
/// every variant is backed by an `Arc`.
#[derive(Clone)]
pub enum ObsColumn {
    /// The shared category labels plus each observation's category code, exactly as
    /// AnnData stores a `categorical` column (a code of `-1` means missing).
    Categorical { categories: Arc<Vec<String>>, codes: Arc<NumericData> },
    /// Numeric values in their stored dtype (expression columns are `Float32`).
    Numeric(NumericData),
}

impl ObsColumn {
    /// Number of observations this column holds values for.
    pub fn len(&self) -> usize {
        match self {
            ObsColumn::Categorical { codes, .. } => codes.len(),
            ObsColumn::Numeric(values) => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The category labels of a `Categorical` column, or `None` for a numeric one.
    pub fn categories(&self) -> Option<&Arc<Vec<String>>> {
        match self {
            ObsColumn::Categorical { categories, .. } => Some(categories),
            ObsColumn::Numeric(_) => None,
        }
    }

    /// Index into [`categories`](Self::categories) for one observation; `None` for a
    /// missing value (code `-1`), an out-of-range code, or a numeric column.
    pub fn category_index_at(&self, row: usize) -> Option<usize> {
        match self {
            ObsColumn::Categorical { categories, codes } => {
                let code = codes.get_f32(row) as i64;
                if code >= 0 && (code as usize) < categories.len() { Some(code as usize) } else { None }
            }
            ObsColumn::Numeric(_) => None,
        }
    }

    /// One observation's value, in the form handed to an [`ObsPredicate`].
    pub fn value_at(&self, row: usize) -> ObsValue<'_> {
        match self {
            ObsColumn::Categorical { categories, .. } => {
                ObsValue::Category(self.category_index_at(row).map(|i| categories[i].as_str()))
            }
            ObsColumn::Numeric(values) => ObsValue::Number(values.get_f32(row)),
        }
    }
}

/// One observation's value for one column, as passed to an [`ObsPredicate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObsValue<'a> {
    /// A categorical value's label, or `None` for a missing value.
    Category(Option<&'a str>),
    Number(f32),
}

impl<'a> ObsValue<'a> {
    /// The category label, or `None` for a missing or non-categorical value.
    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            ObsValue::Category(label) => *label,
            ObsValue::Number(_) => None,
        }
    }

    /// The numeric value, or `None` for a categorical one.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            ObsValue::Number(value) => Some(*value),
            ObsValue::Category(_) => None,
        }
    }
}

/// The loaded values for a set of [`ObsColumnRef`]s, keyed by the reference that
/// requested them. Each distinct reference is read (and memoized) exactly once, even
/// when several stratification levels or predicates share it.
#[derive(Clone, Default)]
pub struct ObsColumns {
    entries: Vec<(ObsColumnRef, ObsColumn)>,
}

impl ObsColumns {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) the values for one column reference.
    pub fn insert(&mut self, column_ref: ObsColumnRef, column: ObsColumn) {
        match self.entries.iter_mut().find(|(existing, _)| *existing == column_ref) {
            Some(entry) => entry.1 = column,
            None => self.entries.push((column_ref, column)),
        }
    }

    pub fn get(&self, column_ref: &ObsColumnRef) -> Option<&ObsColumn> {
        self.entries.iter().find(|(existing, _)| existing == column_ref).map(|(_, column)| column)
    }

    /// Like [`get`](Self::get), but panics: every reference reaching the grouping code
    /// was collected from the same stratification levels and predicates that were
    /// loaded, so a miss is a bug rather than a data problem.
    pub fn expect(&self, column_ref: &ObsColumnRef) -> &ObsColumn {
        self.get(column_ref)
            .unwrap_or_else(|| panic!("Column \"{}\" was not loaded", column_ref.cache_key()))
    }

    /// The obs axis length implied by these columns (the length of the first one), or
    /// `None` when no columns were loaded.
    pub fn n_obs(&self) -> Option<usize> {
        self.entries.first().map(|(_, column)| column.len())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ObsColumnRef, &ObsColumn)> {
        self.entries.iter().map(|(column_ref, column)| (column_ref, column))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Predicates (filtering and selection)
// ---------------------------------------------------------------------------

/// The function behind an [`ObsPredicate`]: it receives one observation's values for
/// the predicate's declared `inputs`, in `inputs` order.
///
/// Split by platform because a trait object may only be bounded by *auto* traits
/// beyond its principal trait: `MaybeSend`/`MaybeSync` are `Send`/`Sync` (auto) on
/// native, but ordinary marker traits on wasm, where they would be rejected here.
/// `ObsPredicate::new` still takes `MaybeSend + MaybeSync` closures on both platforms.
#[cfg(not(target_arch = "wasm32"))]
pub type ObsPredicateFn = dyn Fn(&[ObsValue<'_>]) -> bool + Send + Sync;
#[cfg(target_arch = "wasm32")]
pub type ObsPredicateFn = dyn Fn(&[ObsValue<'_>]) -> bool;

/// An arbitrary per-observation predicate, used either to filter observations out
/// entirely or to select (emphasize) a subset of them.
///
/// `inputs` declares which columns the predicate reads, so that the hooks can load
/// exactly those columns; `test` is then called once per observation with those
/// columns' values for that row. `cache_key` must uniquely identify the predicate
/// *including any values it captures* (e.g. a threshold), since memoized results are
/// keyed on it: two predicates sharing a cache key are assumed to behave identically.
#[derive(Clone)]
pub struct ObsPredicate {
    pub cache_key: String,
    pub inputs: Vec<ObsColumnRef>,
    pub test: Arc<ObsPredicateFn>,
}

impl ObsPredicate {
    pub fn new(
        cache_key: impl Into<String>,
        inputs: Vec<ObsColumnRef>,
        test: impl Fn(&[ObsValue<'_>]) -> bool + MaybeSend + MaybeSync + 'static,
    ) -> Self {
        Self { cache_key: cache_key.into(), inputs, test: Arc::new(test) }
    }

    /// Keeps observations whose `obs[column]` category is one of `allowed`.
    pub fn category_in(column: impl Into<String>, allowed: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let column = column.into();
        let allowed: Vec<String> = allowed.into_iter().map(Into::into).collect();
        let cache_key = format!("category_in({column},[{}])", allowed.join("|"));
        Self::new(cache_key, vec![ObsColumnRef::Categorical(column)], move |values| {
            match values[0].as_str() {
                Some(label) => allowed.iter().any(|candidate| candidate == label),
                None => false,
            }
        })
    }

    /// Keeps observations whose value in `column` falls within `min..=max`. Works for
    /// any numeric column, including a gene's expression column.
    pub fn in_range(column: ObsColumnRef, min: f32, max: f32) -> Self {
        let cache_key = format!("in_range({},{min},{max})", column.cache_key());
        Self::new(cache_key, vec![column], move |values| {
            match values[0].as_f32() {
                Some(value) => value >= min && value <= max,
                None => false,
            }
        })
    }

    /// Keeps observations satisfying both predicates.
    pub fn and(self, other: ObsPredicate) -> Self {
        self.combine("and", other, |left, right| left && right)
    }

    /// Keeps observations satisfying either predicate.
    pub fn or(self, other: ObsPredicate) -> Self {
        self.combine("or", other, |left, right| left || right)
    }

    /// Keeps exactly the observations this predicate rejects.
    pub fn not(self) -> Self {
        let cache_key = format!("not({})", self.cache_key);
        let inputs = self.inputs.clone();
        let test = self.test.clone();
        Self { cache_key, inputs, test: Arc::new(move |values| !test(values)) }
    }

    /// Concatenates the two predicates' inputs and hands each its own slice, so
    /// combinators compose without the operands knowing about each other.
    fn combine(self, name: &str, other: ObsPredicate, join: fn(bool, bool) -> bool) -> Self {
        let cache_key = format!("{name}({},{})", self.cache_key, other.cache_key);
        let split = self.inputs.len();
        let mut inputs = self.inputs.clone();
        inputs.extend(other.inputs.iter().cloned());
        let (left, right) = (self.test.clone(), other.test.clone());
        Self {
            cache_key,
            inputs,
            test: Arc::new(move |values| join(left(&values[..split]), right(&values[split..]))),
        }
    }
}

/// An [`ObsPredicate`] paired with the loaded values of its declared inputs, ready to
/// be evaluated row by row.
struct BoundPredicate<'a> {
    predicate: &'a ObsPredicate,
    inputs: Vec<&'a ObsColumn>,
}

impl<'a> BoundPredicate<'a> {
    fn bind(predicate: &'a ObsPredicate, columns: &'a ObsColumns) -> Self {
        let inputs = predicate.inputs.iter().map(|column_ref| columns.expect(column_ref)).collect();
        Self { predicate, inputs }
    }

    /// `buffer` is owned by the caller purely so that gathering one row's values does
    /// not allocate once per observation.
    fn test(&self, row: usize, buffer: &mut Vec<ObsValue<'a>>) -> bool {
        buffer.clear();
        for column in &self.inputs {
            buffer.push(column.value_at(row));
        }
        (self.predicate.test)(buffer)
    }
}

// ---------------------------------------------------------------------------
// Obs stratification
// ---------------------------------------------------------------------------

/// One level of an obs stratification hierarchy, e.g. cell type, then sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObsStratifyLevel {
    /// Name of a `categorical`-encoded column of `obs`.
    pub column: String,
    /// The subset of the column's categories to keep, in the order they should appear
    /// along this level's axis. `None` keeps every category in stored order;
    /// observations whose category is not kept are excluded from every group.
    pub categories: Option<Vec<String>>,
}

impl ObsStratifyLevel {
    /// Stratifies by every category of `column`, in stored order.
    pub fn new(column: impl Into<String>) -> Self {
        Self { column: column.into(), categories: None }
    }

    /// Stratifies by the given categories of `column` only, in the given order.
    pub fn with_categories(column: impl Into<String>, categories: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            column: column.into(),
            categories: Some(categories.into_iter().map(Into::into).collect()),
        }
    }

    pub fn column_ref(&self) -> ObsColumnRef {
        ObsColumnRef::Categorical(self.column.clone())
    }

    fn cache_key(&self) -> String {
        let categories = match &self.categories {
            Some(categories) => categories.join("|"),
            None => "*".to_string(),
        };
        format!("{}[{categories}]", self.column)
    }
}

/// One resolved level of an [`ObsGrouping`]: which `obs` column it came from, and the
/// categories that form its axis, in order.
#[derive(Clone, Debug)]
pub struct ObsGroupingLevel {
    pub column: String,
    pub categories: Vec<String>,
}

/// The observations belonging to one leaf group of an [`ObsGrouping`], split into the
/// selected ("foreground") and merely-unfiltered ("background") ones.
#[derive(Clone, Debug, Default)]
pub struct ObsGroup {
    /// This group's category label at each stratification level, outermost first.
    /// Empty for an unstratified grouping.
    pub labels: Vec<String>,
    /// This group's category index within each level's `categories`, parallel to
    /// `labels`. Together with [`ObsGrouping::shape`] this locates the group on a grid.
    pub key: Vec<usize>,
    /// Row indices of the observations that passed the filter and are selected.
    pub foreground: Vec<u32>,
    /// Row indices of the observations that passed the filter but are *not* selected,
    /// to be summarized separately and drawn de-emphasized. Always empty when the
    /// grouping had no selection predicate (everything is then foreground).
    pub background: Vec<u32>,
}

impl ObsGroup {
    /// Every observation in the group, selected or not.
    pub fn n_obs(&self) -> usize {
        self.foreground.len() + self.background.len()
    }

    pub fn is_empty(&self) -> bool {
        self.n_obs() == 0
    }
}

/// A stratification of the obs axis: the categories of every level, plus one
/// [`ObsGroup`] per leaf.
///
/// `groups` is the full cartesian product of the levels' categories in row-major order
/// (the innermost level varies fastest), so a group's position on a plot grid follows
/// directly from its index, and empty groups are represented rather than dropped -
/// both of which are what grid layouts (dot plots, faceted violins) need.
pub struct ObsGrouping {
    /// The stratification levels, outermost first. Empty for an unstratified grouping.
    pub levels: Vec<ObsGroupingLevel>,
    /// One group per combination of the levels' categories, row-major.
    pub groups: Vec<ObsGroup>,
    /// Whether a selection predicate was applied, i.e. whether the `background` of the
    /// groups is meaningful (as opposed to "nothing was deselected").
    pub has_selection: bool,
    /// Length of the obs axis these groups were built from.
    pub n_obs: usize,
    /// Observations rejected by the filter predicate.
    pub n_filtered_out: usize,
    /// Observations that passed the filter but belong to no group, because one of
    /// their category values is missing or was dropped by a level's `categories`.
    pub n_unassigned: usize,
}

impl ObsGrouping {
    /// A grouping with no levels and no groups (e.g. when nothing could be loaded).
    pub fn empty() -> Self {
        Self {
            levels: vec![],
            groups: vec![],
            has_selection: false,
            n_obs: 0,
            n_filtered_out: 0,
            n_unassigned: 0,
        }
    }

    /// An unstratified grouping: one group holding all `n_obs` observations, with no
    /// filtering or selection. Needs no I/O, so callers with nothing to stratify by can
    /// use this instead of [`use_obs_grouping`].
    pub fn single_group(n_obs: usize) -> Self {
        Self {
            levels: vec![],
            groups: vec![ObsGroup {
                labels: vec![],
                key: vec![],
                foreground: (0..n_obs as u32).collect(),
                background: vec![],
            }],
            has_selection: false,
            n_obs,
            n_filtered_out: 0,
            n_unassigned: 0,
        }
    }

    /// Number of categories at each level, outermost first; `groups.len()` is its product.
    pub fn shape(&self) -> Vec<usize> {
        self.levels.iter().map(|level| level.categories.len()).collect()
    }

    /// Index into `groups` of the group with the given per-level category indices.
    pub fn group_index(&self, key: &[usize]) -> Option<usize> {
        let shape = self.shape();
        if key.len() != shape.len() {
            return None;
        }
        let mut index = 0usize;
        for (level, &category_index) in key.iter().enumerate() {
            if category_index >= shape[level] {
                return None;
            }
            index = index * shape[level] + category_index;
        }
        Some(index)
    }

    /// One label per group, formed by joining its per-level labels with `separator`
    /// (e.g. `"CD4 T cells / sample_1"`). Suitable as a band-scale domain: the labels
    /// are distinct as long as no category label contains `separator`. An unstratified
    /// grouping yields a single empty label.
    pub fn group_labels(&self, separator: &str) -> Vec<String> {
        self.groups.iter().map(|group| group.labels.join(separator)).collect()
    }
}

/// Assigns every observation to a leaf group, given already-loaded columns.
///
/// Kept separate from [`use_obs_grouping`] (which does the I/O) so that the grouping
/// semantics - hierarchy order, dropped and missing categories, filter vs. selection -
/// can be tested directly, and so that callers already holding the columns can reuse it.
///
/// `n_obs` is the length of the obs axis; pass `None` to infer it from `columns`
/// (needed only when there are no columns at all, in which case there is nothing to
/// infer it from and the result has no observations).
pub fn build_obs_grouping(
    n_obs: Option<usize>,
    levels: &[ObsStratifyLevel],
    filter: Option<&ObsPredicate>,
    selection: Option<&ObsPredicate>,
    columns: &ObsColumns,
) -> ObsGrouping {
    // A column shorter than the obs axis would index out of bounds below, so clamp to
    // the shortest one and say so rather than panicking on malformed data.
    let mut n_rows = n_obs.or_else(|| columns.n_obs()).unwrap_or(0);
    for (column_ref, column) in columns.iter() {
        if column.len() < n_rows {
            log(&format!(
                "build_obs_grouping: column \"{}\" has {} values but the obs axis has {n_rows}; using the shorter length",
                column_ref.cache_key(),
                column.len(),
            ));
            n_rows = column.len();
        }
    }

    // Resolve each level's axis: the categories kept (in axis order), and a lookup from
    // the stored category code to that category's position along the axis (`None` for
    // categories dropped by the level's `categories`).
    struct ResolvedLevel<'a> {
        column: &'a ObsColumn,
        categories: Vec<String>,
        code_to_position: Vec<Option<usize>>,
    }
    let resolved: Vec<ResolvedLevel> = levels
        .iter()
        .map(|level| {
            let column_ref = level.column_ref();
            let column = columns.expect(&column_ref);
            let stored = column.categories().unwrap_or_else(|| {
                panic!("Stratification level \"{}\" must be a categorical obs column", level.column)
            });
            let categories = level.categories.clone().unwrap_or_else(|| stored.as_ref().clone());
            let code_to_position = stored
                .iter()
                .map(|stored_category| categories.iter().position(|kept| kept == stored_category))
                .collect();
            ResolvedLevel { column, categories, code_to_position }
        })
        .collect();

    let shape: Vec<usize> = resolved.iter().map(|level| level.categories.len()).collect();
    // The product of an empty shape is 1, i.e. an unstratified grouping has exactly one
    // group holding every observation, which is the behavior we want.
    let n_groups: usize = shape.iter().product();

    let mut groups: Vec<ObsGroup> = (0..n_groups)
        .map(|flat_index| {
            // Decode the row-major index back into one category index per level.
            let mut key = vec![0usize; shape.len()];
            let mut remainder = flat_index;
            for level in (0..shape.len()).rev() {
                key[level] = remainder % shape[level];
                remainder /= shape[level];
            }
            let labels = key
                .iter()
                .enumerate()
                .map(|(level, &category_index)| resolved[level].categories[category_index].clone())
                .collect();
            ObsGroup { labels, key, foreground: vec![], background: vec![] }
        })
        .collect();

    let bound_filter = filter.map(|predicate| BoundPredicate::bind(predicate, columns));
    let bound_selection = selection.map(|predicate| BoundPredicate::bind(predicate, columns));
    let mut buffer: Vec<ObsValue> = Vec::new();
    let mut n_filtered_out = 0usize;
    let mut n_unassigned = 0usize;

    'rows: for row in 0..n_rows {
        if let Some(predicate) = &bound_filter {
            if !predicate.test(row, &mut buffer) {
                n_filtered_out += 1;
                continue;
            }
        }

        let mut flat_index = 0usize;
        for (level, level_info) in resolved.iter().enumerate() {
            let position = level_info
                .column
                .category_index_at(row)
                .and_then(|code| level_info.code_to_position[code]);
            match position {
                Some(position) => flat_index = flat_index * shape[level] + position,
                None => {
                    n_unassigned += 1;
                    continue 'rows;
                }
            }
        }

        let selected = match &bound_selection {
            Some(predicate) => predicate.test(row, &mut buffer),
            None => true,
        };
        if selected {
            groups[flat_index].foreground.push(row as u32);
        } else {
            groups[flat_index].background.push(row as u32);
        }
    }

    ObsGrouping {
        levels: resolved
            .into_iter()
            .zip(levels)
            .map(|(resolved_level, level)| ObsGroupingLevel {
                column: level.column.clone(),
                categories: resolved_level.categories,
            })
            .collect(),
        groups,
        has_selection: selection.is_some(),
        n_obs: n_rows,
        n_filtered_out,
        n_unassigned,
    }
}

// ---------------------------------------------------------------------------
// Hooks: single-element reads
// ---------------------------------------------------------------------------

/// Loads (and caches) `var.index`, i.e. the gene IDs.
pub async fn use_var_index(store: AdataStore, store_name: &str, cache_enabled: bool) -> Result<Arc<Vec<String>>, ArrayError> {
    let deps = vec!["adata_var_index".to_string(), store_name.to_string()];
    use_memo_vec_string(async || read_dataframe_index(store.clone(), "/var").await, &deps, cache_enabled).await
}

/// Loads (and caches) `obs.index`, i.e. the cell IDs.
pub async fn use_obs_index(store: AdataStore, store_name: &str, cache_enabled: bool) -> Result<Arc<Vec<String>>, ArrayError> {
    let deps = vec!["adata_obs_index".to_string(), store_name.to_string()];
    use_memo_vec_string(async || read_dataframe_index(store.clone(), "/obs").await, &deps, cache_enabled).await
}

/// Loads (and caches) a `categorical`-encoded column of `obs` as its category labels
/// and per-observation codes. The two arrays are cached (and fetched) independently,
/// since the labels are small and change far less often than any downstream summary.
pub async fn use_obs_categorical(
    store: AdataStore,
    store_name: &str,
    column: &str,
    cache_enabled: bool,
) -> Result<(Arc<Vec<String>>, Arc<NumericData>), ArrayError> {
    let column_path = format!("/obs/{column}");

    let categories_deps = vec!["adata_obs_categories".to_string(), store_name.to_string(), column.to_string()];
    let categories_future = use_memo_vec_string(
        async || read_string_array(store.clone(), &format!("{column_path}/categories")).await,
        &categories_deps,
        cache_enabled,
    );

    let codes_deps = vec!["adata_obs_codes".to_string(), store_name.to_string(), column.to_string()];
    let codes_future = use_memo_numeric_data(
        async || load_arr_as_numeric_data(store.clone(), &format!("{column_path}/codes")).await,
        &codes_deps,
        cache_enabled,
    );

    futures::try_join!(categories_future, codes_future)
}

/// Loads (and caches) a numeric (`array`-encoded) column of `obs`, in its stored dtype.
// TODO: handle the nullable-integer / nullable-boolean encodings, which store
// `values` and `mask` subarrays rather than a bare array.
pub async fn use_obs_numeric(
    store: AdataStore,
    store_name: &str,
    column: &str,
    cache_enabled: bool,
) -> Result<Arc<NumericData>, ArrayError> {
    let deps = vec!["adata_obs_numeric".to_string(), store_name.to_string(), column.to_string()];
    use_memo_numeric_data(
        async || load_arr_as_numeric_data(store.clone(), &format!("/obs/{column}")).await,
        &deps,
        cache_enabled,
    )
    .await
}

/// Path of the expression matrix for `layer`: `/X` for `None`, else `/layers/<layer>`.
pub fn expr_array_path(layer: Option<&str>) -> String {
    match layer {
        None => "/X".to_string(),
        Some(layer) => format!("/layers/{layer}"),
    }
}

/// Loads (and caches) one gene's column of an expression matrix, by column index into
/// `var.index`. Cached per (matrix, column), independently of any grouping or cutoff,
/// so that changing those does not re-fetch expression values.
// TODO: support sparse (csr_matrix / csc_matrix) expression matrices; this assumes dense.
pub async fn use_expression_column(
    store: AdataStore,
    store_name: &str,
    layer: Option<&str>,
    col_index: u64,
    cache_enabled: bool,
) -> Result<Arc<Vec<f32>>, ArrayError> {
    let array_path = expr_array_path(layer);
    let deps = vec![
        "adata_expr_column".to_string(),
        store_name.to_string(),
        array_path.clone(),
        col_index.to_string(),
    ];
    use_memo_vec_f32(
        async || read_dense_column_f32(store.clone(), &array_path, col_index).await,
        &deps,
        cache_enabled,
    )
    .await
}

// ---------------------------------------------------------------------------
// Hooks: sets of columns, and groupings
// ---------------------------------------------------------------------------

/// Loads every column in `refs`, concurrently and deduplicated, so that a column named
/// by several stratification levels or predicates is read only once.
pub async fn use_obs_columns(
    store: AdataStore,
    store_name: &str,
    refs: &[ObsColumnRef],
    cache_enabled: bool,
) -> Result<ObsColumns, ArrayError> {
    let mut unique: Vec<ObsColumnRef> = Vec::new();
    for column_ref in refs {
        if !unique.contains(column_ref) {
            unique.push(column_ref.clone());
        }
    }

    // Expression columns are named by gene, so resolving them needs `var.index` first.
    let var_index = if unique.iter().any(|column_ref| matches!(column_ref, ObsColumnRef::Expression { .. })) {
        Some(use_var_index(store.clone(), store_name, cache_enabled).await?)
    } else {
        None
    };

    let column_futures = unique.iter().map(|column_ref| {
        let store = store.clone();
        let var_index = var_index.clone();
        async move { load_obs_column(store, store_name, column_ref, var_index.as_deref(), cache_enabled).await }
    });
    let loaded = futures::future::try_join_all(column_futures).await?;

    Ok(ObsColumns { entries: unique.into_iter().zip(loaded).collect() })
}

async fn load_obs_column(
    store: AdataStore,
    store_name: &str,
    column_ref: &ObsColumnRef,
    var_index: Option<&Vec<String>>,
    cache_enabled: bool,
) -> Result<ObsColumn, ArrayError> {
    match column_ref {
        ObsColumnRef::Categorical(column) => {
            let (categories, codes) = use_obs_categorical(store, store_name, column, cache_enabled).await?;
            Ok(ObsColumn::Categorical { categories, codes })
        }
        ObsColumnRef::Numeric(column) => {
            let values = use_obs_numeric(store, store_name, column, cache_enabled).await?;
            Ok(ObsColumn::Numeric(values.as_ref().clone()))
        }
        ObsColumnRef::Expression { layer, var_name } => {
            // Unlike the plotted genes (which are skipped when missing, since a plot
            // with fewer genes is still meaningful), a gene a predicate reads is not
            // optional: silently treating it as absent would silently change which
            // observations are kept or emphasized.
            let var_index = var_index.expect("var.index must be loaded before resolving an expression column");
            let col_index = var_index
                .iter()
                .position(|name| name == var_name)
                .unwrap_or_else(|| panic!("Gene \"{var_name}\" (referenced by a predicate) not found in var.index"));
            let values = use_expression_column(store, store_name, layer.as_deref(), col_index as u64, cache_enabled).await?;
            Ok(ObsColumn::Numeric(NumericData::Float32(values)))
        }
    }
}

/// Loads (and caches) the [`ObsGrouping`] for one stratification hierarchy, filter and
/// selection: the columns those need are loaded (each independently memoized) and then
/// grouped by [`build_obs_grouping`], with the resulting grouping memoized as a unit.
///
/// `n_obs` is the length of the obs axis. Prefer passing the length of whatever array
/// the groups will be used to index into (e.g. an expression column), so that every row
/// index in a group is valid there; `None` infers it from the loaded columns instead.
/// An unstratified, unfiltered, unselected grouping has no columns of its own at all -
/// use [`ObsGrouping::single_group`] for that case, which needs no I/O.
pub async fn use_obs_grouping(
    store: AdataStore,
    store_name: &str,
    n_obs: Option<usize>,
    levels: &[ObsStratifyLevel],
    filter: Option<&ObsPredicate>,
    selection: Option<&ObsPredicate>,
    cache_enabled: bool,
) -> Result<Arc<ObsGrouping>, ArrayError> {
    let keys = vec![
        "adata_obs_grouping".to_string(),
        store_name.to_string(),
        n_obs.map(|n| n.to_string()).unwrap_or_else(|| "infer".to_string()),
        levels.iter().map(|level| level.cache_key()).collect::<Vec<_>>().join(","),
        filter.map(|predicate| predicate.cache_key.clone()).unwrap_or_default(),
        selection.map(|predicate| predicate.cache_key.clone()).unwrap_or_default(),
    ];

    use_memo_obs_grouping(
        async || {
            let mut refs: Vec<ObsColumnRef> = levels.iter().map(|level| level.column_ref()).collect();
            for predicate in [filter, selection].into_iter().flatten() {
                refs.extend(predicate.inputs.iter().cloned());
            }
            let columns = use_obs_columns(store.clone(), store_name, &refs, cache_enabled).await?;
            Ok(build_obs_grouping(n_obs, levels, filter, selection, &columns))
        },
        &keys,
        cache_enabled,
    )
    .await
}

// ---------------------------------------------------------------------------
// Summaries
// ---------------------------------------------------------------------------

/// Summary of one gene's expression distribution over one set of observations - the
/// two quantities a dot plot encodes (color and size), plus the group size behind them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExpressionSummary {
    /// Number of observations summarized; 0 means the group was empty and the
    /// summarized values below are 0.0 (i.e. an absent dot rather than a real one).
    pub n_obs: u32,
    /// Mean expression across those observations.
    pub mean: f32,
    /// Fraction of those observations whose expression exceeds the cutoff.
    pub fraction_expressing: f32,
}

/// Summarizes one gene's expression (a whole column of the expression matrix) over the
/// given observation rows, matching scanpy's `sc.pl.dotplot` semantics: the mean over
/// the rows, and the fraction of them strictly above `expression_cutoff`.
pub fn summarize_expression(values: &[f32], rows: &[u32], expression_cutoff: f32) -> ExpressionSummary {
    if rows.is_empty() {
        return ExpressionSummary::default();
    }
    let mut sum = 0.0f32;
    let mut n_expressing = 0usize;
    for &row in rows {
        let value = values[row as usize];
        sum += value;
        if value > expression_cutoff {
            n_expressing += 1;
        }
    }
    ExpressionSummary {
        n_obs: rows.len() as u32,
        mean: sum / rows.len() as f32,
        fraction_expressing: n_expressing as f32 / rows.len() as f32,
    }
}

// ---------------------------------------------------------------------------
// Hook: dot plot data
// ---------------------------------------------------------------------------

/// Everything that determines the data behind one dot plot: which expression values to
/// summarize, how to stratify the obs axis, and which observations to drop or emphasize.
#[derive(Clone, Default)]
pub struct DotPlotQuery {
    /// Expression matrix to read: `None` for `X`, or a `layers` key.
    pub layer: Option<String>,
    /// Gene IDs from `var.index`, in the order they should appear along the gene axis.
    /// Genes not present in `var.index` are logged and skipped.
    pub var_names: Vec<String>,
    /// Obs stratification hierarchy, outermost level first; a dot is produced per
    /// (leaf group, gene) pair. Empty means one group holding every observation.
    pub stratify_by: Vec<ObsStratifyLevel>,
    /// Observations this rejects contribute to nothing at all.
    pub filter: Option<ObsPredicate>,
    /// Observations this rejects are summarized separately, as the background.
    pub selection: Option<ObsPredicate>,
    /// A gene counts as expressed in an observation when its value exceeds this.
    pub expression_cutoff: f32,
}

impl DotPlotQuery {
    fn cache_keys(&self, store_name: &str) -> Vec<String> {
        vec![
            "adata_dotplot_data".to_string(),
            store_name.to_string(),
            self.layer.clone().unwrap_or_else(|| "X".to_string()),
            self.var_names.join(","),
            self.stratify_by.iter().map(|level| level.cache_key()).collect::<Vec<_>>().join(","),
            self.filter.as_ref().map(|predicate| predicate.cache_key.clone()).unwrap_or_default(),
            self.selection.as_ref().map(|predicate| predicate.cache_key.clone()).unwrap_or_default(),
            self.expression_cutoff.to_string(),
        ]
    }
}

/// Separator used to join a group's per-level category labels into its axis tick label.
pub const GROUP_LABEL_SEPARATOR: &str = " / ";

/// One dot of a dot plot: which group and gene it summarizes, and the summarized
/// values. Each dot knows where it belongs, so drawing (or picking) a dot never needs
/// to reconstruct its position from an index.
#[derive(Clone, Debug)]
pub struct DotPlotDot {
    /// Position along the group axis, i.e. index into [`DotPlotData::group_labels`]
    /// (and into [`ObsGrouping::groups`]).
    pub group_index: usize,
    /// Position along the gene axis, i.e. index into [`DotPlotData::var_names`].
    pub gene_index: usize,
    /// Summary over the group's selected observations: what the dot itself encodes.
    pub foreground: ExpressionSummary,
    /// Summary over the group's unselected (but unfiltered) observations, to be drawn
    /// de-emphasized behind the foreground. `None` when the query had no selection
    /// predicate, i.e. when every unfiltered observation is in the foreground.
    pub background: Option<ExpressionSummary>,
}

/// The dots of one dot plot, plus the two axes they are positioned on.
pub struct DotPlotData {
    /// Gene axis: the subset of the requested gene IDs that were found in `var.index`,
    /// in the same relative order as requested. May be shorter than requested.
    pub var_names: Vec<String>,
    /// Group axis: one tick label per leaf group of the obs stratification, joined with
    /// [`GROUP_LABEL_SEPARATOR`] when the stratification has more than one level.
    pub group_labels: Vec<String>,
    /// The stratification the dots were grouped by. Carries what the group axis labels
    /// leave out: the levels, each group's per-level category, and how many
    /// observations were filtered out or left unassigned.
    pub grouping: Arc<ObsGrouping>,
    /// One dot per (group, gene) pair, in group-major order.
    pub dots: Vec<DotPlotDot>,
}

impl DotPlotData {
    pub fn n_genes(&self) -> usize {
        self.var_names.len()
    }

    pub fn n_groups(&self) -> usize {
        self.group_labels.len()
    }

    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.dots.is_empty()
    }

    // These return `&String` rather than `&str` so that they can be handed straight to
    // a `ScaleBand`, whose domain values are `String`s.

    /// The gene axis label of a dot, i.e. its gene's ID in `var.index`.
    pub fn var_name(&self, dot: &DotPlotDot) -> &String {
        &self.var_names[dot.gene_index]
    }

    /// The group axis label of a dot.
    pub fn group_label(&self, dot: &DotPlotDot) -> &String {
        &self.group_labels[dot.group_index]
    }

    /// The group a dot summarizes, e.g. for its per-level category labels or its
    /// observation indices.
    pub fn group(&self, dot: &DotPlotDot) -> &ObsGroup {
        &self.grouping.groups[dot.group_index]
    }

    /// The dot for one (group, gene) pair.
    pub fn dot(&self, group_index: usize, gene_index: usize) -> Option<&DotPlotDot> {
        if gene_index >= self.n_genes() {
            return None;
        }
        self.dots.get(group_index * self.n_genes() + gene_index)
    }
}

/// Loads (and caches) the [`DotPlotData`] for one dot plot configuration.
///
/// Nests two levels of caching: the individual zarr reads (var index, groupby
/// categories/codes, per-gene expression columns) are each cached independently via
/// `pluot_core::cache`'s `use_memo_*` (so e.g. a stratification change doesn't re-fetch
/// gene expression columns that are still valid), and the grouping is cached as a unit
/// of its own (so e.g. an `expression_cutoff` change doesn't re-group the obs axis),
/// while the derived summaries are cached keyed on every parameter that affects them.
pub async fn use_dotplot_data(
    store: AdataStore,
    store_name: &str,
    query: &DotPlotQuery,
    cache_enabled: bool,
) -> Result<Arc<DotPlotData>, ArrayError> {
    let keys = query.cache_keys(store_name);
    use_memo_dotplot_data(
        async || load_dotplot_data(store.clone(), store_name, query, cache_enabled).await,
        &keys,
        cache_enabled,
    )
    .await
}

async fn load_dotplot_data(
    store: AdataStore,
    store_name: &str,
    query: &DotPlotQuery,
    cache_enabled: bool,
) -> Result<DotPlotData, ArrayError> {
    // Resolve the requested gene IDs to their column index in `var.index`, logging
    // (and skipping) any that aren't found.
    let var_index = use_var_index(store.clone(), store_name, cache_enabled).await?;
    let var_index_lookup: HashMap<&str, usize> =
        var_index.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();
    let mut var_names: Vec<String> = Vec::new();
    let mut gene_col_indices: Vec<u64> = Vec::new();
    for var_name in &query.var_names {
        match var_index_lookup.get(var_name.as_str()) {
            Some(&i) => {
                var_names.push(var_name.clone());
                gene_col_indices.push(i as u64);
            }
            None => log(&format!("use_dotplot_data: gene \"{var_name}\" not found in var.index; skipping")),
        }
    }

    if var_names.is_empty() {
        return Ok(DotPlotData {
            var_names,
            group_labels: vec![],
            grouping: Arc::new(ObsGrouping::empty()),
            dots: vec![],
        });
    }

    let layer = query.layer.as_deref();
    let column_futures = gene_col_indices.iter().map(|&col_index| {
        let store = store.clone();
        async move { use_expression_column(store, store_name, layer, col_index, cache_enabled).await }
    });

    // The expression columns are loaded first, and the grouping only afterwards, rather
    // than joining the two: under `wait_for_store_gets: false` a pending read returns a
    // `TimedOut` error immediately, and a `try_join!` would react to it by dropping the
    // other branch mid-read, cancelling in-flight store requests. Awaiting in sequence
    // means a bail-out never cancels anything, and each read is memoized, so the reads
    // that did land are still there on the next render pass.
    let columns: Vec<Arc<Vec<f32>>> = futures::future::try_join_all(column_futures).await?;

    // Grouping against the obs axis length of the expression matrix (rather than
    // letting the grouping infer it from its own columns) keeps every row index in a
    // group valid for the columns summarized below, even if `obs` and the expression
    // matrix disagree about how many observations there are.
    let n_obs = columns.first().map_or(0, |column| column.len());
    let is_unstratified = query.stratify_by.is_empty() && query.filter.is_none() && query.selection.is_none();
    let grouping: Arc<ObsGrouping> = if is_unstratified {
        // Nothing to stratify by, filter, or select on: one group of every observation,
        // which needs no I/O at all.
        Arc::new(ObsGrouping::single_group(n_obs))
    } else {
        use_obs_grouping(
            store.clone(),
            store_name,
            Some(n_obs),
            &query.stratify_by,
            query.filter.as_ref(),
            query.selection.as_ref(),
            cache_enabled,
        )
        .await?
    };

    // Summarize each (group, gene) distribution: the foreground always, and the
    // background too whenever a selection made the distinction meaningful.
    let mut dots: Vec<DotPlotDot> = Vec::with_capacity(grouping.groups.len() * var_names.len());
    for (group_index, group) in grouping.groups.iter().enumerate() {
        for (gene_index, values) in columns.iter().enumerate() {
            dots.push(DotPlotDot {
                group_index,
                gene_index,
                foreground: summarize_expression(values, &group.foreground, query.expression_cutoff),
                background: grouping
                    .has_selection
                    .then(|| summarize_expression(values, &group.background, query.expression_cutoff)),
            });
        }
    }

    Ok(DotPlotData {
        var_names,
        group_labels: grouping.group_labels(GROUP_LABEL_SEPARATOR),
        grouping,
        dots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A categorical obs column with the given labels and per-observation codes
    /// (`-1` for a missing value, as AnnData stores it).
    fn categorical(categories: &[&str], codes: &[i32]) -> ObsColumn {
        ObsColumn::Categorical {
            categories: Arc::new(categories.iter().map(|c| c.to_string()).collect()),
            codes: Arc::new(NumericData::Int32(Arc::new(codes.to_vec()))),
        }
    }

    fn numeric(values: &[f32]) -> ObsColumn {
        ObsColumn::Numeric(NumericData::Float32(Arc::new(values.to_vec())))
    }

    /// Two levels over 6 cells: cell_type in {B, T} and sample in {s1, s2}.
    fn two_level_columns() -> ObsColumns {
        let mut columns = ObsColumns::new();
        columns.insert(
            ObsColumnRef::Categorical("cell_type".to_string()),
            categorical(&["B", "T"], &[0, 0, 1, 1, 1, 0]),
        );
        columns.insert(
            ObsColumnRef::Categorical("sample".to_string()),
            categorical(&["s1", "s2"], &[0, 1, 0, 1, 1, -1]),
        );
        columns
    }

    fn foregrounds(grouping: &ObsGrouping) -> Vec<Vec<u32>> {
        grouping.groups.iter().map(|group| group.foreground.clone()).collect()
    }

    #[test]
    fn single_level_grouping_uses_stored_category_order() {
        let columns = two_level_columns();
        let levels = vec![ObsStratifyLevel::new("cell_type")];
        let grouping = build_obs_grouping(None, &levels, None, None, &columns);

        assert_eq!(grouping.n_obs, 6);
        assert_eq!(grouping.group_labels("/"), vec!["B", "T"]);
        assert_eq!(foregrounds(&grouping), vec![vec![0, 1, 5], vec![2, 3, 4]]);
        assert!(grouping.groups.iter().all(|group| group.background.is_empty()));
        assert!(!grouping.has_selection);
        assert_eq!((grouping.n_filtered_out, grouping.n_unassigned), (0, 0));
    }

    #[test]
    fn hierarchy_is_the_cartesian_product_innermost_level_fastest() {
        let columns = two_level_columns();
        let levels = vec![ObsStratifyLevel::new("cell_type"), ObsStratifyLevel::new("sample")];
        let grouping = build_obs_grouping(None, &levels, None, None, &columns);

        assert_eq!(grouping.shape(), vec![2, 2]);
        assert_eq!(grouping.group_labels("/"), vec!["B/s1", "B/s2", "T/s1", "T/s2"]);
        // Cell 5 is a B cell with a missing sample, so it belongs to no group.
        assert_eq!(foregrounds(&grouping), vec![vec![0], vec![1], vec![2], vec![3, 4]]);
        assert_eq!(grouping.n_unassigned, 1);

        // Keys and indices agree in both directions.
        for (index, group) in grouping.groups.iter().enumerate() {
            assert_eq!(grouping.group_index(&group.key), Some(index));
        }
        assert_eq!(grouping.group_index(&[1, 0]), Some(2));
        assert_eq!(grouping.group_index(&[2, 0]), None);
        assert_eq!(grouping.group_index(&[0]), None);
    }

    #[test]
    fn level_categories_subset_and_reorder_the_axis() {
        let columns = two_level_columns();
        let levels = vec![ObsStratifyLevel::with_categories("cell_type", ["T"])];
        let grouping = build_obs_grouping(None, &levels, None, None, &columns);

        assert_eq!(grouping.group_labels("/"), vec!["T"]);
        assert_eq!(foregrounds(&grouping), vec![vec![2, 3, 4]]);
        // The three B cells are dropped rather than filtered, so they are unassigned.
        assert_eq!(grouping.n_unassigned, 3);

        let reordered = vec![ObsStratifyLevel::with_categories("cell_type", ["T", "B"])];
        let grouping = build_obs_grouping(None, &reordered, None, None, &columns);
        assert_eq!(grouping.group_labels("/"), vec!["T", "B"]);
        assert_eq!(foregrounds(&grouping), vec![vec![2, 3, 4], vec![0, 1, 5]]);
    }

    #[test]
    fn no_levels_yields_one_group_of_every_observation() {
        let columns = two_level_columns();
        let grouping = build_obs_grouping(None, &[], None, None, &columns);
        assert_eq!(foregrounds(&grouping), vec![vec![0, 1, 2, 3, 4, 5]]);
        assert_eq!(grouping.group_labels("/"), vec![""]);

        // Without any columns to infer from, the obs axis length must be given.
        let grouping = build_obs_grouping(None, &[], None, None, &ObsColumns::new());
        assert_eq!(grouping.n_obs, 0);
        assert_eq!(foregrounds(&grouping), vec![Vec::<u32>::new()]);
        assert_eq!(foregrounds(&ObsGrouping::single_group(3)), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn filtering_excludes_and_selection_splits() {
        let mut columns = two_level_columns();
        columns.insert(ObsColumnRef::Numeric("n_counts".to_string()), numeric(&[10.0, 20.0, 30.0, 40.0, 50.0, 60.0]));

        let levels = vec![ObsStratifyLevel::new("cell_type")];
        // Filter out cell 0 (n_counts below 20), then select the s1 cells.
        let filter = ObsPredicate::in_range(ObsColumnRef::Numeric("n_counts".to_string()), 20.0, f32::INFINITY);
        let selection = ObsPredicate::category_in("sample", ["s1"]);
        let grouping = build_obs_grouping(None, &levels, Some(&filter), Some(&selection), &columns);

        assert!(grouping.has_selection);
        assert_eq!(grouping.n_filtered_out, 1);
        assert_eq!(foregrounds(&grouping), vec![Vec::<u32>::new(), vec![2]]);
        let backgrounds: Vec<Vec<u32>> = grouping.groups.iter().map(|group| group.background.clone()).collect();
        assert_eq!(backgrounds, vec![vec![1, 5], vec![3, 4]]);
        // Filtered-out observations are in neither, and both are still counted in the group.
        assert_eq!(grouping.groups.iter().map(|group| group.n_obs()).sum::<usize>(), 5);
    }

    #[test]
    fn predicate_combinators_slice_their_operands_inputs() {
        let mut columns = two_level_columns();
        columns.insert(ObsColumnRef::Numeric("n_counts".to_string()), numeric(&[10.0, 20.0, 30.0, 40.0, 50.0, 60.0]));

        let is_t = ObsPredicate::category_in("cell_type", ["T"]);
        let is_high = ObsPredicate::in_range(ObsColumnRef::Numeric("n_counts".to_string()), 40.0, f32::INFINITY);

        let both = is_t.clone().and(is_high.clone());
        assert_eq!(both.inputs.len(), 2);
        assert!(both.cache_key.contains("and("));

        let levels: Vec<ObsStratifyLevel> = vec![];
        let grouping = build_obs_grouping(None, &levels, Some(&both), None, &columns);
        assert_eq!(foregrounds(&grouping), vec![vec![3, 4]]);

        let either = is_t.clone().or(is_high.clone());
        let grouping = build_obs_grouping(None, &levels, Some(&either), None, &columns);
        assert_eq!(foregrounds(&grouping), vec![vec![2, 3, 4, 5]]);

        let grouping = build_obs_grouping(None, &levels, Some(&is_t.not()), None, &columns);
        assert_eq!(foregrounds(&grouping), vec![vec![0, 1, 5]]);
    }

    #[test]
    fn predicates_can_read_gene_expression() {
        let mut columns = ObsColumns::new();
        columns.insert(ObsColumnRef::Categorical("cell_type".to_string()), categorical(&["B", "T"], &[0, 0, 1, 1]));
        columns.insert(ObsColumnRef::expression("CD79A"), numeric(&[0.0, 5.0, 0.0, 7.0]));

        let expressing = ObsPredicate::in_range(ObsColumnRef::expression("CD79A"), 0.1, f32::INFINITY);
        let levels = vec![ObsStratifyLevel::new("cell_type")];
        let grouping = build_obs_grouping(None, &levels, Some(&expressing), None, &columns);
        assert_eq!(foregrounds(&grouping), vec![vec![1], vec![3]]);
    }

    #[test]
    fn summarize_expression_matches_dotplot_semantics() {
        let values = [0.0, 1.0, 2.0, 9.0];

        let summary = summarize_expression(&values, &[0, 1, 2], 0.0);
        assert_eq!(summary.n_obs, 3);
        assert_eq!(summary.mean, 1.0);
        // The cutoff is exclusive, so the 0.0 value does not count as expressing.
        assert_eq!(summary.fraction_expressing, 2.0 / 3.0);

        // Only the given rows contribute, in any order.
        let summary = summarize_expression(&values, &[3, 1], 1.5);
        assert_eq!(summary.mean, 5.0);
        assert_eq!(summary.fraction_expressing, 0.5);

        // An empty group is all zeros rather than NaN.
        assert_eq!(summarize_expression(&values, &[], 0.0), ExpressionSummary::default());
    }

    #[test]
    fn shorter_column_than_obs_axis_is_clamped() {
        let mut columns = ObsColumns::new();
        columns.insert(ObsColumnRef::Categorical("cell_type".to_string()), categorical(&["B"], &[0, 0]));
        let levels = vec![ObsStratifyLevel::new("cell_type")];
        let grouping = build_obs_grouping(Some(5), &levels, None, None, &columns);
        assert_eq!(grouping.n_obs, 2);
        assert_eq!(foregrounds(&grouping), vec![vec![0, 1]]);
    }
}
