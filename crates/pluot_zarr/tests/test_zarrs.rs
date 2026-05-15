// We only run this test on non-WASM targets.
#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use std::sync::Arc;
use std::error::Error;
use zarrs::filesystem::FilesystemStore;

// Reference: https://github.com/zarrs/zarrs/blob/main/zarrs_storage/tests/adapters.rs

use zarrs::storage::storage_adapter::async_to_sync::{
    AsyncToSyncBlockOn, AsyncToSyncStorageAdapter,
};
use zarrs::storage::storage_adapter::sync_to_async::{
    SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter,
};
use zarrs::storage::store::MemoryStore;

struct TokioSpawnBlocking;

impl SyncToAsyncSpawnBlocking for TokioSpawnBlocking {
    async fn spawn_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        tokio::task::spawn_blocking(f).await.unwrap()
    }
}

struct TokioBlockOn(tokio::runtime::Runtime);

impl AsyncToSyncBlockOn for TokioBlockOn {
    fn block_on<F: core::future::Future>(&self, future: F) -> F::Output {
        self.0.block_on(future)
    }
}

#[tokio::test]
async fn test_async_read_array_subset() -> Result<(), Box<dyn Error>> {
    let store = Arc::new(FilesystemStore::new("../../data/out/6001240_labels.ome.zarr")
        .expect("Create filesystem store"));
    let store = Arc::new(SyncToAsyncStorageAdapter::new(store, TokioSpawnBlocking));

    let lowres_array = zarrs::array::Array::async_open(store.clone(), "/2")
        .await
        .expect("Open lowres dataset array");

    let slice_99_subset = zarrs::array::ArraySubset::new_with_ranges(&[
        0..1, 99..100, 0..68, 0..67
    ]);

    let arr = lowres_array.async_retrieve_array_subset::<Vec<u16>>(&slice_99_subset)
        .await
        .expect("Read pixel data");

    println!("Read array with shape {:?} and dtype i16", arr.len());

    println!("Reading array subset3: {:?}", lowres_array.subset_all());





    Ok(())

}


#[test]
fn test_read_array_subset() {

    let store = Arc::new(FilesystemStore::new("../../data/out/6001240_labels.ome.zarr")
        .expect("Create filesystem store"));

    let lowres_array = zarrs::array::Array::open(store.clone(), "/2")
        .expect("Open lowres dataset array");

    println!("Reading array subset3: {:?}", lowres_array.subset_all());

    let arr = lowres_array.retrieve_array_subset::<Vec<u16>>(&lowres_array.subset_all())
        .expect("Read pixel data");

    println!("Read array with shape {:?} and dtype i16", arr.len());

    let slice_99_subset = zarrs::array::ArraySubset::new_with_ranges(&[
        0..1, 99..100, 0..68, 0..67
    ]);

    let arr = lowres_array.retrieve_array_subset::<Vec<u16>>(&slice_99_subset)
        .expect("Read pixel data");

    println!("Read array with shape {:?} and dtype i16", arr.len());

    let img_h = 68;
    let img_w = 67;

    // This array is CZYX.
    // TODO: do not assume 4D and dim order.
    let arr_subset = zarrs::array::ArraySubset::new_with_ranges(&[
        0..1, 0..1, 0..img_h as u64, 0..img_w as u64,
    ]);

    println!("Reading array subset: {:?}", arr_subset);

    let arr_subset2 = zarrs::array::ArraySubset::new_with_ranges(&[
        0..1, 99..100, 0..img_h as u64, 0..img_w as u64,
    ]);

    println!("Reading array subset2: {:?}", arr_subset2);

    /*new_with_start_end_exc(
        vec![0, 99, 0, 0], // start
        vec![1, 100, img_h as u64, img_w as u64], // end, exclusive
    ).expect("Compatible dimensionality");
*/

    /* ::new_with_start_shape(
        vec![0, 0, 0, 0], // start
        vec![1, 1, img_h as u64, img_w as u64], // shape
    ).expect("Compatible dimensionality");*/

    // TODO: support other dtypes.
    let arr = lowres_array.retrieve_array_subset::<Vec<u16>>(&arr_subset)
        .expect("Read pixel data");

    println!("Read array with shape {:?} and dtype i16", arr.len());

    let arr2 = lowres_array.retrieve_array_subset::<Vec<u16>>(&arr_subset2)
        .expect("Read pixel data2");

    println!("Read array with shape {:?} and dtype i16", arr2.len());

    assert_eq!(arr.len(), img_h as usize * img_w as usize);
    assert_eq!(arr2.len(), img_h as usize * img_w as usize);


}
