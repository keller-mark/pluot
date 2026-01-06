use crate::wgpu;
use std::borrow::Cow;
use std::mem::size_of;

// Prototyping an API for layered plotting.

// Constants from deck.gl-native
// TODO: Update casing of names.
// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/webgpu/src/webgpu-constants.h#L46
const kMaxBindGroups: u32 = 4;

// TODO: investigate bindgroup limits
const kMaxBindingsPerGroup: u32 = 16;
const kMaxVertexAttributes: u32 = 16;
// Comments copied
// Vulkan has a standalone limit named maxVertexInputAttributeOffset (2047u at least) for vertex
// attribute offset. The limit might be meaningless because Vulkan has another limit named
// maxVertexInputBindingStride (2048u at least). We use maxVertexAttributeEnd (2048u) here to
// verify vertex attribute offset, which equals to maxOffset + smallest size of vertex format
// (char). We may use maxVertexInputBindingStride (maxVertexBufferStride below) instead to replace
// maxVertexAttributeEnd in future.
const kMaxVertexAttributeEnd: u32 = 2048;
const kMaxVertexBuffers: u32 = 16;
const kMaxVertexBufferStride: u32 = 2048;
const kNumStages: u32 = 3;
const kMaxColorAttachments: u32 = 4;
const kTextureRowPitchAlignment: u32 = 256;
// Dynamic buffer offsets require offset to be divisible by 256
const kMinDynamicBufferOffsetAlignment: u64 = 256;
// Max numbers of dynamic uniform buffers
const kMaxDynamicUniformBufferCount: u32 = 8;
// Max numbers of dynamic storage buffers
const kMaxDynamicStorageBufferCount: u32 = 4;
// Max numbers of dynamic buffers
const kMaxDynamicBufferCount: u32 = kMaxDynamicUniformBufferCount + kMaxDynamicStorageBufferCount;
// Indirect command sizes
const kDispatchIndirectSize: u64 = 3 * size_of::<u32>() as u64;
const kDrawIndirectSize: u64 = 4 * size_of::<u32>() as u64;
const kDrawIndexedIndirectSize: u64 = 5 * size_of::<u32>() as u64;

// Non spec defined constants.
const kLodMin: f32 = 0.0;
const kLodMax: f32 = 1000.0;

// Max texture size constants
const kMaxTextureSize: u32 = 8192;
const kMaxTexture2DArrayLayers: u32 = 256;
const kMaxTexture2DMipLevels: u32 = 14;

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
    field_type: wgpu::VertexFormat,
}

struct TableSchema {
    num_rows: u32,
    fields: Vec<TableField>,
}

struct Table {
    schema: TableSchema,
}

struct UniformDescriptor {
    pub shader_stage: wgpu::ShaderStages,
    // In deck.gl-native, binding_types are only UniformBuffer (default), Sampler, or SampledTexture.
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

// Structure with one constructor per-type of bindings, so that the initializer_list accepts
// bindings with the right type and no extra information.
// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/webgpu/src/webgpu-helpers.h#L102
struct BindingInitializationHelper {
    binding: u32,
    // TODO: split into three separate structs, and only allow sampler OR texture_view OR buffer+offset+size?
    sampler: Option<wgpu::Sampler>,
    texture_view: Option<wgpu::TextureView>,
    buffer: Option<wgpu::Buffer>,
    offset: u64,
    size: u64,
}

trait GetAsBinding {
    fn get_as_binding(&self) -> wgpu::BindGroupEntry;
}

impl GetAsBinding for BindingInitializationHelper {
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/webgpu/src/webgpu-helpers.cc#L232
    fn get_as_binding(&self) -> wgpu::BindGroupEntry {
        let resource: wgpu::BindingResource = match (self.sampler, self.texture_view, self.buffer) {
            (Some(sampler), None, None) => wgpu::BindingResource::Sampler(sampler),
            (None, Some(texture_view), None) => wgpu::BindingResource::TextureView(texture_view),
            (None, None, Some(buffer)) => wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: self.offset,
                size: self.size,
            }),
            _ => panic!("Invalid binding initialization"),
        };
        wgpu::BindGroupEntry {
            binding: self.binding,
            resource,
        }
    }
}

// References:
// - https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.h#L50
// - https://github.com/visgl/luma.gl/blob/master/modules/engine/src/model/model.ts
// - https://github.com/visgl/luma.gl/tree/master/modules/webgpu/src
struct Model {
    pub device: wgpu::Device,
    pub options: ModelOptions,

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

    attribute_table: Table,
    instanced_attribute_table: Table,
    indices: Option<Vec<u32>>, // TODO: is this the type we want to use here?

    // Some things to keep track of, from ComboVertexStateDescriptor and ComboRenderPipelineDescriptor.
    vertex_buffer_count: u32,
    c_vertex_buffers: Vec<wgpu::VertexBufferLayout>,
    c_attributes: Vec<wgpu::VertexAttribute>,

    // _bindings
    bindings: Vec<Option<BindingInitializationHelper>>,
}

fn create_bind_group_layout(device: &wgpu::Device, uniforms: &[UniformDescriptor]) -> wgpu::BindGroupLayout {
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L142
    let mut bindings: Vec<wgpu::BindGroupLayoutEntry> = Vec::with_capacity(uniforms.len());
    for i in 0..uniforms.len() {
        bindings.push(wgpu::BindGroupLayoutEntry {
            binding: i as u32,
            visibility: uniforms[i].shader_stage,
            // Assume the binding_type sets `has_dynamic_offset` correctly.
            ty: uniforms[i].binding_type,
            count: None,
        });
    }
    // Filter out any bindings with visibility == ShaderStages::NONE (not visible from any stages).
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/webgpu/src/webgpu-helpers.cc#L196
    let filtered_bindings = bindings.into_iter().filter(|entry| entry.visibility != wgpu::ShaderStages::NONE).collect::<Vec<_>>();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Uniform Bind Group Layout"),
        entries: &filtered_bindings,
    })
}

fn make_basic_pipeline_layout(device: &wgpu::Device, bind_group_layout: Option<&wgpu::BindGroupLayout>) -> wgpu::PipelineLayout {
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/webgpu/src/webgpu-helpers.cc#L177
    if let Some(bind_group_layout) = bind_group_layout {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Basic Pipeline Layout - Count is 1"),
            bind_group_layouts: &[bind_group_layout],
            immediate_size: 0,
        })
    } else {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Basic Pipeline Layout - Count is 0"),
            bind_group_layouts: &[],
            immediate_size: 0,
        })
    }
}

impl Model {
    pub fn new(device: wgpu::Device, options: ModelOptions) -> Self {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L34

        // Create shader modules.
        let vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex Shader"),
            source: wgpu::ShaderSource::Wgsl(options.vs.into()),
        });
        let fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment Shader"),
            source: wgpu::ShaderSource::Wgsl(options.fs.into()),
        });

        // Create render pipeline descriptor.

        // Initialize vertex state.
        // (Inlined _initializeVertexState, since not used anywhere else)
        let vertex_buffer_count = options.attribute_schema.fields.len() + options.instanced_attribute_schema.fields.len();
        let mut location = 0;
        let mut c_vertex_buffers: Vec<wgpu::VertexBufferLayout> = Vec::with_capacity(kMaxVertexBuffers as usize);
        let mut c_attributes: Vec<wgpu::VertexAttribute> = Vec::with_capacity(kMaxVertexAttributes as usize);
        for attribute_field in options.attribute_schema.fields {
            let vertex_format_size = attribute_field.field_type.size();
            let vertex_attribute = wgpu::VertexAttribute {
                offset: 0,
                shader_location: location as u32,
                format: attribute_field.field_type,
            };
            c_vertex_buffers[location] = wgpu::VertexBufferLayout {
                array_stride: vertex_format_size,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[vertex_attribute],
            };
            c_attributes[location] = vertex_attribute;
            location += 1;
        }
        for attribute_field in options.instanced_attribute_schema.fields {
            let vertex_format_size = attribute_field.field_type.size();
            let vertex_attribute = wgpu::VertexAttribute {
                offset: 0,
                shader_location: location as u32,
                format: attribute_field.field_type,
            };
            c_vertex_buffers[location] = wgpu::VertexBufferLayout {
                array_stride: vertex_format_size,
                step_mode: wgpu::VertexStepMode::Instance, // Main difference from above for loop.
                attributes: &[vertex_attribute],
            };
            c_attributes[location] = vertex_attribute;
            location += 1;
        }

        // Initialize uniform cache (this.bindings)
        let bindings: Vec<Option<BindingInitializationHelper>> = Vec::with_capacity(options.uniforms.len());

        // Set uniformBindGroupLayout (this._createBindGroupLayout())
        // Create bind group layout.
        let uniform_bind_group_layout = create_bind_group_layout(&device, &options.uniforms);

        // Create pipeline layout (makeBasicPipelineLayout).
        let layout = make_basic_pipeline_layout(&device, Some(&uniform_bind_group_layout));

        // Create the RenderPipelineDescriptor down here (since depends on creating the pipeline layout)
        let descriptor = wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &c_vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L47
                    format: options.texture_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: options.primitive_topology,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        };

        let pipeline = device.create_render_pipeline(&descriptor);
        Model {
            device,
            options,
            vs_module,
            fs_module,
            // We do not yet set the bind group.
            // This gets set in the Model::_setBinding method.
            bind_group: None,
            pipeline,
            uniform_bind_group_layout,
            // Initialize with empty tables?
            attribute_table: Table {
                schema: TableSchema {
                    num_rows: 0,
                    fields: Vec::new(),
                },
            },
            instanced_attribute_table: Table {
                schema: TableSchema {
                    num_rows: 0,
                    fields: Vec::new(),
                },
            },
            indices: None,
            vertex_buffer_count: vertex_buffer_count as u32,
            c_vertex_buffers,
            c_attributes,
            bindings,
        }
    }

    pub fn set_attributes(&mut self, attributes: Table) {
        self.attribute_table = attributes;
    }
    pub fn set_instanced_attributes(&mut self, attributes: Table) {
        self.instanced_attribute_table = attributes;
    }

    pub fn set_indices(&mut self, indices: Option<Vec<u32>>) {
        self.indices = indices;
    }
    pub fn set_uniform_buffer(&mut self, binding: u32, buffer: wgpu::Buffer, offset: u64, size: u64) {
        self.set_binding(binding, BindingInitializationHelper {
            binding,
            buffer: Some(buffer),
            offset,
            size,
            texture_view: None,
            sampler: None,
        })
    }

    pub fn set_uniform_texture(&mut self, binding: u32, texture_view: wgpu::TextureView) {
        self.set_binding(binding, BindingInitializationHelper {
            binding,
            buffer: None,
            offset: 0,
            size: 0,
            texture_view: Some(texture_view),
            sampler: None,
        })
    }

    pub fn set_uniform_sampler(&mut self, binding: u32, sampler: wgpu::Sampler) {
        self.set_binding(binding, BindingInitializationHelper {
            binding,
            buffer: None,
            offset: 0,
            size: 0,
            texture_view: None,
            sampler: Some(sampler),
        })
    }

    pub fn draw(&mut self, pass: &mut wgpu::RenderPass) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L91C46-L91C47
        pass.set_pipeline(&self.pipeline);
        self.set_vertex_buffers(pass);
        // The argument is used for specifying dynamic offsets, which is not something we support right now.
        pass.set_bind_group(0, &self.bind_group, &[]);

        let vertex_count = self.attribute_table.schema.num_rows;

        let min_instances = 1;
        let instance_count = std::cmp::max(self.instanced_attribute_table.schema.num_rows, min_instances);
        if let Some(indices) = self.indices {
            pass.set_index_buffer(self.indices);
            pass.draw_indexed(self.indices.length(), instance_count);
        } else {
            pass.draw(vertex_count, instance_count);
        }
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
