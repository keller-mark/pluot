use crate::wgpu;
use std::borrow::Cow;

// Prototyping an API for layered plotting.

struct Layer {

}

// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/layers/src/scatterplot-layer/scatterplot-layer.h#L35
struct ScatterplotLayer {
    x_data: Option<Vec<f32>>,
    y_data: Option<Vec<f32>>,
    labels_data: Option<Vec<i32>>,
}

struct BitmapLayer {

}

struct CompositeLayer {

}

struct TileLayer {

}

// TODO: Should this diverge more from deck.gl-native which was tied to Arrow table schema representations?
// Perhaps we want something closer to a Zarr array schema representation?
// Stepping back, what is DeckGL-native using its Schema/Field structs to achieve?
// - Setting up one vertex buffer per Field
//     Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L117
//   - Each buffer/field has a `type` which is used by
//       arrayStride = lumagl::garrow::getVertexFormatSize(field->type())
//       format = field->type()
//    This is done for the Fields of both attributeSchema and instancedAttributeSchema.
// - Getting the number of rows of the attributeTable and instancedAttributeTable
//   Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L97

// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/garrow/src/schema.h
// Sequence of Field objects describing the columns of a table data structure.
struct TableField {
    name: String,
    // TODO: add a `type` property?
    // getVertexFormatSize(field->type())
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/garrow/src/util/webgpu-utils.cc#L28
}

struct TableSchema {
    num_rows: u32,
    fields: Vec<TableField>,
}

struct UniformDescriptor {
    pub shader_stage: wgpu::ShaderStages,
    pub binding_type: wgpu::BindingType,
    pub is_dynamic: bool,
}

struct ModelOptions {
    // Vertex shader source
    // TODO: only allow a single string for both vs and fs, since wgsl supports vertex/fragment in same string?
    pub vs: String,
    // Fragment shader source
    pub fs: String,
    // Attribute definitions.
    pub attribute_schema: TableSchema,
    // Instanced attribute definitions.
    pub instanced_attribute_schema:  TableSchema,
    //  Uniform definitions.
    pub uniforms: Vec<UniformDescriptor>,
    // Type of geometry topology that will be contained in vertex buffers.
    pub primitive_topology: wgpu::PrimitiveTopology,
    // Texture format that the pipeline will use.
    pub texture_format: wgpu::TextureFormat,
}

impl Default for ModelOptions {
    fn default() -> Self {
        ModelOptions {
            vs: String::new(),
            fs: String::new(),
            attribute_schema: TableSchema {
                num_rows: 0,
                fields: Vec::new(),
            },
            instanced_attribute_schema: TableSchema {
                num_rows: 0,
                fields: Vec::new(),
            },
            uniforms: Vec::new(),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            texture_format: wgpu::TextureFormat::Bgra8Unorm,
        }
    }
}

// References:
// - https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.h#L50
// - https://github.com/visgl/luma.gl/blob/master/modules/engine/src/model/model.ts
// - https://github.com/visgl/luma.gl/tree/master/modules/webgpu/src
struct Model {
    pub device: wgpu::Device,
    pub model_options: ModelOptions,

    // Rendering pipeline.
    pub pipeline: wgpu::RenderPipeline,
    // Layout of the bind group.
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    // Bind group containg uniform data.
    pub bind_group: Option<wgpu::BindGroup>,
    // TODO: use a single shader module for both vertex and fragment shaders? See above comment in ModelOptions.
    // Compiled vertex shader.
    pub vs_module: wgpu::ShaderModule,
    // Compiled fragment shader.
    pub fs_module: wgpu::ShaderModule,
}

impl Model {
    pub fn new(device: wgpu::Device, model_options: ModelOptions) -> Self {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L34

        // Create shader modules.
        let vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex Shader"),
            source: wgpu::ShaderSource::Wgsl(model_options.vs.into()),
        });
        let fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment Shader"),
            source: wgpu::ShaderSource::Wgsl(model_options.fs.into()),
        });

        // Create bind group layout.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Render Pipeline Bind Group Layout"),
            entries: &[],
        });

        // Create pipeline layout (makeBasicPipelineLayout).
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        // Create render pipeline descriptor.
        let descriptor = wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.texture_desc.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        };

        // Initialize vertex state.

        // Initialize uniform cache (this.bindings)

        // Set uniformBindGroupLayout (this._createBindGroupLayout())


        Model {
            device,
            model_options,
            vs_module,
            fs_module,
            // We do not yet set the bind group.
            // This gets set in the Model::_setBinding method.
            bind_group: None,
        }
    }

    pub fn set_attributes(&mut self) {
        // TODO
    }
    pub fn set_instanced_attributes(&mut self) {
        // TODO
    }

    pub fn set_indices(&mut self) {
        // TODO
    }
    pub fn set_uniform_buffer(&mut self) {
        // TODO
    }

    pub fn set_uniform_texture(&mut self) {
        // TODO
    }

    pub fn set_uniform_sampler(&mut self) {
        // TODO
    }

    pub fn draw(&mut self, pass: &mut wgpu::RenderPass) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L91C46-L91C47
        // TODO

        let vertex_count = self.attribute_table.num_rows;

        let min_instances = 1;
        let instance_count = std::cmp::max(self.instanced_attribute_table.num_rows, min_instances);
        if let Some(indices) = self.indices {
            pass.set_index_buffer(self.indices.buffer());
            pass.draw_indexed(self.indices.length(), instance_count);
        } else {
            pass.draw(vertex_count, instance_count);
        }
    }

    fn initialize_vertex_state() {
        // TODO
    }

    fn create_bind_group_layout(device: &wgpu::Device, uniforms: &[UniformDescriptor]) -> wgpu::BindGroupLayout {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L142
        // TODO
    }

    fn set_binding(&mut self, binding: u32, init_helper: ) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L154C49-L154C76
        // TODO
    }

    fn set_vertex_buffers(&mut self, pass: &mut wgpu::RenderPass) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L171
        // TODO

    }
}

pub trait GetModel {
    /// Given a value from the domain, returns the corresponding value in the range.
    fn get_model(&self, device: &wgpu::Device) -> Model;
}

impl GetModel for ScatterplotLayer {
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/deck.gl/layers/src/scatterplot-layer/scatterplot-layer.cc#L205
    fn get_model(&self, device: &wgpu::Device) -> Model {
        Model::new(device)
    }
}

pub async fn render_layered_plot(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
    // Get x and y data from the Zarr store.
    let store = context.store;
    let height = context.params.height as f64;
    let width = context.params.width as f64;

    let margin_top = context.params.margin_top.unwrap_or(0.0) as f64;
    let margin_right = context.params.margin_right.unwrap_or(0.0) as f64;
    let margin_bottom = context.params.margin_bottom.unwrap_or(0.0) as f64;
    let margin_left = context.params.margin_left.unwrap_or(0.0) as f64;


    let scatterplot_layer = ScatterplotLayer::new(device);
}
