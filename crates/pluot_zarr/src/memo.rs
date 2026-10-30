use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    static USE_MEMO_CACHE_SHARED: RefCell<HashMap<Vec<String>, Box<dyn Any>>> = RefCell::new(HashMap::new());
}

/// Memoizes the result of `initializer` under `keys`, for any type. Each caller's keys must
/// start with a namespace of its own, since a cached value of another type under the same
/// keys is recomputed (and replaced) rather than returned.
pub async fn use_memo_shared<T: 'static>(initializer: impl AsyncFnOnce() -> T, keys: &[String], cache_enabled: bool) -> Arc<T> {
    if !cache_enabled {
        return Arc::new(initializer().await);
    }
    let cached = USE_MEMO_CACHE_SHARED.with(|cache| cache.borrow().get(keys).and_then(|value| value.downcast_ref::<Arc<T>>().cloned()));
    if let Some(value) = cached {
        return value;
    }
    let value = Arc::new(initializer().await);
    USE_MEMO_CACHE_SHARED.with(|cache| {
        cache.borrow_mut().insert(keys.to_vec(), Box::new(value.clone()));
    });
    value
}
