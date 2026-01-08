use std::sync::Arc;
use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};

use crate::deckish::layer::{DrawToCanvas, PreparedLayer, ViewParams};
use crate::deckish::model::{
    Model, ModelOptions, SpecialArray, Table, TableField, TableSchema, UniformDescriptor
};
use crate::wgpu;
use crate::zarr::AsyncZarritaStore;
use crate::cache::get_or_init_buffer;


#[derive(ShaderType, Debug)]
struct ScatterplotUniforms {
    viewport_size: Vec2, // (width, height) in pixels
    plot_margin: Vec4,   // (top, right, bottom, left) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    point_size_px: f32,  // diameter in pixels
    color: Vec4,         // rgba color for points
}

struct ScatterplotData {
    x_arr: Vec<f32>,
    y_arr: Vec<f32>,
    labels_arr: Vec<i32>,
}

pub struct ScatterplotLayer {
    view_params: ViewParams,
    // TODO: do we want the store or just the store_name here?
    store: Arc<AsyncZarritaStore>,
    store_name: String,
    layer_id: String,
    x_key: String,
    y_key: String,
    color_key: Option<String>,
    point_radius: Option<f32>,
    // Data will be None prior to runninng prepare().
    data: Option<ScatterplotData>,
}

impl ScatterplotLayer {
    pub fn new(
        view_params: ViewParams,
        store: Arc<AsyncZarritaStore>,
        store_name: String,
        layer_id: String,
        x_key: String,
        y_key: String,
        color_key: Option<String>,
        point_radius: Option<f32>,
    ) -> Self {
        Self {
            view_params,
            store,
            store_name,
            layer_id,
            x_key,
            y_key,
            color_key,
            point_radius,
            data: None,
        }
    }

    async fn get_model(&self, device: wgpu::Device, queue: wgpu::Queue) -> Model {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        let x_bytes = bytemuck::cast_slice(&data.x_arr);
        let y_bytes = bytemuck::cast_slice(&data.y_arr);

        // More efficient version that eliminates intermediate vectors and redundant operations
        let n = data.labels_arr.len();

        // Convert to f32 and cast to bytes directly - no for loop needed
        let labels_i32: Vec<i32> = data.labels_arr.iter().map(|&c| c as i32).collect();
        let labels_bytes: &[u8] = bytemuck::cast_slice(&labels_i32);


        // Create buffers
        // TODO(clone): refactor SpecialArray to take references instead of cloning device/queue.
        // It gets tricky since we need to consider the Buffer lifetime, which we may want to be shorter.
        let mut x_special_array = SpecialArray::new(device.clone(), queue.clone());
        x_special_array.set_data(x_bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, None);
        let x_buffer = x_special_array.get_buffer();

        let mut y_special_array = SpecialArray::new(device.clone(), queue.clone());
        y_special_array.set_data(y_bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, None);
        let y_buffer = y_special_array.get_buffer();

        let mut labels_special_array = SpecialArray::new(device.clone(), queue.clone());
        labels_special_array.set_data(labels_bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, None);
        let labels_buffer = labels_special_array.get_buffer();

        // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
        let camera_view = self.view_params.camera_view.unwrap_or([
            // Column 0
            1.0, 0.0, 0.0, 0.0, // Column 1
            0.0, 1.0, 0.0, 0.0, // Column 2
            0.0, 0.0, 1.0, 0.0, // Column 3
            0.0, 0.0, 0.0, 1.0,
        ]);

        let margin_top = self.view_params.margin_top.unwrap_or(0.0) as f64;
        let margin_right = self.view_params.margin_right.unwrap_or(0.0) as f64;
        let margin_bottom = self.view_params.margin_bottom.unwrap_or(0.0) as f64;
        let margin_left = self.view_params.margin_left.unwrap_or(0.0) as f64;

        let point_size_px: f32 = self.point_radius.unwrap_or(5.0);

        let viewport_w = self.view_params.width as f32;
        let viewport_h = self.view_params.height as f32;

        // Construct the uniform struct using Encase.
        let uniform_struct = ScatterplotUniforms {
            camera_view: Mat4::from_cols_array(&camera_view),
            plot_margin: Vec4::from_array([
                // top, right, bottom, left
                margin_top as f32,
                margin_right as f32,
                margin_bottom as f32,
                margin_left as f32,
            ]),
            point_size_px,
            viewport_size: Vec2::new(viewport_w, viewport_h),
            color: Vec4::from_array([1.0, 0.0, 0.0, 1.0]),
        };

        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(&uniform_struct).unwrap();
        let uniform_bytes = buffer.into_inner();

        let mut u_special_array = SpecialArray::new(device.clone(), queue.clone());
        u_special_array.set_data(&uniform_bytes, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, None);
        let u_buffer = u_special_array.get_buffer();

        let shader_source = wgpu::include_wgsl!("scatterplot_layer.wgsl");

        let options = ModelOptions {
            shader_source,
            attribute_schema: TableSchema {
                // TODO: should this attribute_schema value be different?
                num_rows: 0 as u32,
                fields: Vec::new(),
            },
            instanced_attribute_schema: TableSchema {
                // TODO: should this instanced_attribute_schema value be different?
                num_rows: n as u32,
                fields: vec![
                    TableField {
                        name: "x".to_string(),
                        field_type: wgpu::VertexFormat::Float32,
                    },
                    TableField {
                        name: "y".to_string(),
                        field_type: wgpu::VertexFormat::Float32,
                    },
                    TableField {
                        name: "labels".to_string(),
                        field_type: wgpu::VertexFormat::Sint32,
                    },
                ],
            },
            // Define the uniform descriptors.
            uniforms: vec![
                // The uniforms buffer.
                UniformDescriptor {
                    shader_stage: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    binding_type: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                // The X coordinates buffer.
                UniformDescriptor {
                    shader_stage: wgpu::ShaderStages::VERTEX,
                    binding_type: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                // The Y coordinates buffer.
                UniformDescriptor {
                    shader_stage: wgpu::ShaderStages::VERTEX,
                    binding_type: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                // The class labels coordinates buffer.
                UniformDescriptor {
                    shader_stage: wgpu::ShaderStages::FRAGMENT,
                    binding_type: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
            ],
            primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
            texture_format: wgpu::TextureFormat::Rgba8UnormSrgb,
        };

        let mut model = Model::new(device, options);

        // TODO(clone): refactor set_uniform_buffer to take references.
        // It converts things to references internally anyway.
        model.set_uniform_buffer(0, u_buffer.clone(), 0, uniform_bytes.len() as u64);
        model.set_uniform_buffer(1, x_buffer.clone(), 0, x_bytes.len() as u64);
        model.set_uniform_buffer(2, y_buffer.clone(), 0, y_bytes.len() as u64);
        model.set_uniform_buffer(3, labels_buffer.clone(), 0, labels_bytes.len() as u64);

        return model;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ScatterplotLayer {
    async fn prepare(&mut self) {
        let store = self.store.clone();

        let height = self.view_params.height as f64;
        let width = self.view_params.width as f64;

        let margin_top = self.view_params.margin_top.unwrap_or(0.0) as f64;
        let margin_right = self.view_params.margin_right.unwrap_or(0.0) as f64;
        let margin_bottom = self.view_params.margin_bottom.unwrap_or(0.0) as f64;
        let margin_left = self.view_params.margin_left.unwrap_or(0.0) as f64;

        let labels_array_path = self.color_key.as_ref().expect("Color key");

        let labels_array_future = zarrs::array::Array::async_open(store.clone(), labels_array_path);

        // Wait for all futures to complete
        //let arr_open_results = futures::join!(labels_array_future);

        let labels_array = labels_array_future.await.unwrap();

        let labels_subset = labels_array.subset_all();

        // Use futures::join! to run the async retrievals in parallel, similar to Promise.all in JS.
        let labels_result = labels_array.async_retrieve_array_subset_ndarray::<i64>(&labels_subset).await;

        // Print the Zarr.json metadata to the JS console.
        // log(&x_array.metadata().to_string_pretty());

        // Read the whole array
        let labels_vec = labels_result.unwrap();

        // More efficient version that eliminates intermediate vectors and redundant operations
        // Convert to f32 and cast to bytes directly - no for loop needed
        let labels_i32: Vec<i32> = labels_vec.iter().map(|&c| c as i32).collect();

        // TODO: improve the keys / memoization dependencies to at least include the plot_id and store_name.
        let x_f32_future_deps = vec!["x_bytes".to_string(), self.store_name.to_string(), self.layer_id.to_string()];
        let x_f32_future = get_or_init_buffer(async || {
            let x_array_path = &self.x_key.as_ref();
            let x_array_future = zarrs::array::Array::async_open(store.clone(), x_array_path);
            let x_array = x_array_future.await.unwrap();
            let x_subset = x_array.subset_all();
            let x_result = x_array.async_retrieve_array_subset_ndarray::<f64>(&x_subset).await;

            let x_vec = x_result.unwrap();
            let x_f32_inner: Vec<f32> = x_vec.iter().map(|&x| x as f32).collect();
            x_f32_inner
        }, &x_f32_future_deps, self.view_params.cache_enabled);

        let y_f32_future_deps = vec!["y_bytes".to_string(), self.store_name.to_string(), self.layer_id.to_string()];
        let y_f32_future = get_or_init_buffer(async || {
            let y_array_path = &self.y_key.as_ref();
            let y_array_future = zarrs::array::Array::async_open(store.clone(), y_array_path);
            let y_array = y_array_future.await.unwrap();
            let y_subset = y_array.subset_all();
            let y_result = y_array.async_retrieve_array_subset_ndarray::<f64>(&y_subset).await;

            let y_vec = y_result.unwrap();
            let y_f32_inner: Vec<f32> = y_vec.iter().map(|&y| y as f32).collect();
            y_f32_inner
        }, &y_f32_future_deps, self.view_params.cache_enabled);

        // Await in parallel.
        let (x_f32, y_f32) = futures::join!(x_f32_future, y_f32_future);

        self.data = Some(ScatterplotData {
            x_arr: x_f32,
            y_arr: y_f32,
            labels_arr: labels_i32,
        });
    }
}
