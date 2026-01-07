use crate::wgpu;
use std::mem::size_of;
use std::num::{NonZero, NonZeroU64};

// Port of the LumaGL/DeckGL Model class.
// The Model is an abstraction over a WGPU render pipeline and helps with buffer management.
// Reference: https://luma.gl/docs/api-reference/engine/model

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
#[derive(Clone)]
pub struct TableField {
    name: String,
    // TODO: add a `type` property?
    // getVertexFormatSize(field->type())
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/garrow/src/util/webgpu-utils.cc#L28
    field_type: wgpu::VertexFormat,
}
#[derive(Clone)]
pub struct TableSchema {
    num_rows: u32,
    fields: Vec<TableField>,
}
#[derive(Clone)]
pub struct Table {
    schema: TableSchema,
    columns: Vec<SpecialArray>,
}

// Array data structure that manages the backing GPU buffer.
// Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/garrow/src/array.h#L35
#[derive(Clone)]
pub struct SpecialArray {
    //data: Option<Vec<u32>>, // TODO: make this generic over any numeric type?
    device: wgpu::Device,
    queue: wgpu::Queue,
    buffer: Option<wgpu::Buffer>,
    length: i64,
    buffer_byte_size: u64,
    index_format: Option<wgpu::IndexFormat>,
}

impl SpecialArray {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            //data: None,
            device,
            queue,
            buffer: None,
            length: 0,
            buffer_byte_size: 0,
            index_format: None,
        }
    }
    pub fn length(&self) -> i64 {
        self.length
    }
    // TODO: make generic to support other data types.
    pub fn set_data(&mut self, data: Vec<u8>, usage: wgpu::BufferUsages, index_format: Option<wgpu::IndexFormat>) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/garrow/src/array.h#L58
        let buffer_byte_size = data.len() as u64 * std::mem::size_of::<u8>() as u64;
        if self.buffer.is_none() || self.buffer_byte_size != buffer_byte_size {
            self.buffer = Some(self.create_buffer(buffer_byte_size, usage));
        }
        self.queue.write_buffer(
            &self.buffer.as_ref().expect("Buffer not initialized"),
            0, &data
        );
        self.length = data.len() as i64;
        self.buffer_byte_size = buffer_byte_size;
        self.index_format = index_format;
    }
    pub fn create_buffer(&self, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage,
            mapped_at_creation: false,
        })
    }
    // Returns the backing buffer that this array manages.
    pub fn get_buffer(&self) -> &wgpu::Buffer {
        self.buffer.as_ref().expect("Buffer not initialized")
    }
    pub fn get_index_format(&self) -> wgpu::IndexFormat {
        self.index_format.expect("Index format not initialized")
    }

}

#[derive(Clone)]
pub struct UniformDescriptor {
    pub shader_stage: wgpu::ShaderStages,
    // In deck.gl-native, binding_types are only UniformBuffer (default), Sampler, or SampledTexture.
    pub binding_type: wgpu::BindingType,
    // pub is_dynamic: bool, // Never used because the Buffer variant of wgpu::BindingType has its own property has_dynamic_offset
}

#[derive(Clone)]
pub struct ModelOptions {
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
#[derive(Clone)]
struct BindingInitializationHelper {
    binding: u32,
    // TODO: split into three separate structs, and only allow sampler OR texture_view OR buffer+offset+size?
    sampler: Option<wgpu::Sampler>,
    texture_view: Option<wgpu::TextureView>,
    buffer: Option<wgpu::Buffer>,
    offset: u64,
    size: Option<NonZero<u64>>,
}

trait GetAsBinding {
    fn get_as_binding(&self) -> wgpu::BindGroupEntry;
}

impl GetAsBinding for BindingInitializationHelper {
    // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/webgpu/src/webgpu-helpers.cc#L232
    fn get_as_binding(&self) -> wgpu::BindGroupEntry {
        let resource: wgpu::BindingResource = match (&self.sampler, &self.texture_view, &self.buffer) {
            (Some(sampler), None, None) => wgpu::BindingResource::Sampler(&sampler),
            (None, Some(texture_view), None) => wgpu::BindingResource::TextureView(&texture_view),
            (None, None, Some(buffer)) => wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
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
pub struct Model {
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
    indices: Option<SpecialArray>,

    // Some things to keep track of, from ComboVertexStateDescriptor and ComboRenderPipelineDescriptor.
    pub vertex_buffer_count: u32,
    //c_vertex_buffers: Vec<wgpu::VertexBufferLayout<'a>>,
    //c_attributes: Vec<wgpu::VertexAttribute>,

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

fn make_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, bindings: Vec<BindingInitializationHelper>) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Basic Bind Group"),
        layout,
        entries: &bindings.iter().map(|binding| binding.get_as_binding()).collect::<Vec<_>>(),
    })
}

// Reference: https://github.com/gfx-rs/wgpu/discussions/1790#discussioncomment-5969750
struct VertexStuff {
    vertex_attributes: [wgpu::VertexAttribute; 1],
    layout_array_stride: u64,
    layout_step_mode: wgpu::VertexStepMode,
}
impl VertexStuff {
    fn new(layout_array_stride: u64, layout_step_mode: wgpu::VertexStepMode, attribute_offset: u64, attribute_location: u32, attribute_format: wgpu::VertexFormat) -> Self {
        Self {
            vertex_attributes: [
                wgpu::VertexAttribute {
                    offset: attribute_offset,
                    shader_location: attribute_location,
                    format: attribute_format,
                }
            ],
            layout_array_stride,
            layout_step_mode,
        }
    }
    fn get_vertex_buffer_layout(&self) -> wgpu::VertexBufferLayout<'_> {
        wgpu::VertexBufferLayout {
            array_stride: self.layout_array_stride,
            step_mode: self.layout_step_mode,
            attributes: &self.vertex_attributes,
        }
    }
}

impl Model {
    pub fn new(device: wgpu::Device, options: ModelOptions) -> Self {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L34

        // Create shader modules.
        let vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex Shader"),
            source: wgpu::ShaderSource::Wgsl(options.vs.clone().into()),
        });
        let fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment Shader"),
            source: wgpu::ShaderSource::Wgsl(options.fs.clone().into()),
        });

        // Create render pipeline descriptor.

        // Initialize vertex state.
        // (Inlined _initializeVertexState, since not used anywhere else)
        let vertex_buffer_count = options.attribute_schema.fields.len() + options.instanced_attribute_schema.fields.len();
        let mut location = 0;
        let mut c_vertex_stuff: Vec<VertexStuff> = Vec::with_capacity(kMaxVertexBuffers as usize);
        for attribute_field in &options.attribute_schema.fields {
            let vertex_format_size = attribute_field.field_type.size();
            c_vertex_stuff.push(VertexStuff::new(vertex_format_size, wgpu::VertexStepMode::Vertex, 0, location as u32, attribute_field.field_type));
            location += 1;
        }
        for attribute_field in &options.instanced_attribute_schema.fields {
            let vertex_format_size = attribute_field.field_type.size();
            c_vertex_stuff.push(VertexStuff::new(vertex_format_size, wgpu::VertexStepMode::Instance, 0, location as u32, attribute_field.field_type));
            location += 1;
        }
        let c_vertex_buffers = c_vertex_stuff.iter().map(|vs| vs.get_vertex_buffer_layout()).collect::<Vec<_>>();

        // Initialize uniform cache (this.bindings)
        let bindings: Vec<Option<BindingInitializationHelper>> = vec![None; options.uniforms.len()];

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
        Self {
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
                columns: Vec::new(),
            },
            instanced_attribute_table: Table {
                schema: TableSchema {
                    num_rows: 0,
                    fields: Vec::new(),
                },
                columns: Vec::new(),
            },
            indices: None,
            vertex_buffer_count: vertex_buffer_count as u32,
            //c_vertex_buffers,
            //c_attributes,
            bindings,
        }
    }

    pub fn set_attributes(&mut self, attributes: Table) {
        self.attribute_table = attributes;
    }
    pub fn set_instanced_attributes(&mut self, attributes: Table) {
        self.instanced_attribute_table = attributes;
    }

    pub fn set_indices(&mut self, indices: SpecialArray) {
        self.indices = Some(indices);
    }
    pub fn set_uniform_buffer(&mut self, binding: u32, buffer: wgpu::Buffer, offset: u64, size: u64) {
        self.set_binding(binding, BindingInitializationHelper {
            binding,
            buffer: Some(buffer),
            offset,
            size: NonZeroU64::new(size),
            texture_view: None,
            sampler: None,
        })
    }

    pub fn set_uniform_texture(&mut self, binding: u32, texture_view: wgpu::TextureView) {
        self.set_binding(binding, BindingInitializationHelper {
            binding,
            buffer: None,
            offset: 0,
            size: None,
            texture_view: Some(texture_view),
            sampler: None,
        })
    }

    pub fn set_uniform_sampler(&mut self, binding: u32, sampler: wgpu::Sampler) {
        self.set_binding(binding, BindingInitializationHelper {
            binding,
            buffer: None,
            offset: 0,
            size: None,
            texture_view: None,
            sampler: Some(sampler),
        })
    }

    pub fn draw(&mut self, pass: &mut wgpu::RenderPass) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L91C46-L91C47
        // See also JS equivalent: https://github.com/visgl/luma.gl/blob/6f43c54bf3b6a83f8a4fe3bfb90b46098e74681b/modules/engine/src/model/model.ts#L401
        pass.set_pipeline(&self.pipeline);
        self.set_vertex_buffers(pass);
        // The argument is used for specifying dynamic offsets, which is not something we support right now.
        pass.set_bind_group(0, &self.bind_group, &[]);

        let vertex_count = self.attribute_table.schema.num_rows;

        let min_instances = 1;
        let instance_count = std::cmp::max(self.instanced_attribute_table.schema.num_rows, min_instances);
        if let Some(indices) = &self.indices {
            pass.set_index_buffer(indices.get_buffer().slice(..), indices.get_index_format());
            pass.draw_indexed(0..indices.length() as u32, 0, 0..instance_count);
        } else {
            pass.draw(0..vertex_count, 0..instance_count);
        }
    }

    fn set_binding(&mut self, binding: u32, init_helper: BindingInitializationHelper) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L154C49-L154C76
        self.bindings[binding as usize] = Some(init_helper);

        // Make sure all uniforms are set before trying to create a bind group

        // Filter out bindings that are None
        let non_empty_bindings: Vec<BindingInitializationHelper> = self.bindings.iter()
            .filter_map(|binding| binding.clone())
            .collect();

        // We need the number of items in uniform_bind_group_layout to match the number of elements in non_empty_bindings.
        // Therefore, we first check that the number of non-empty bindings matches the number of entries in the layout.
        // Note: the cpp code does not appear to do such a check. Unclear why/how.
        if non_empty_bindings.len() != self.options.uniforms.len() {
            // Not all bindings are set yet, so we cannot create the bind group.
            return;
        }
        self.bind_group = Some(make_bind_group(&self.device, &self.uniform_bind_group_layout, non_empty_bindings));
    }

    fn set_vertex_buffers(&mut self, pass: &mut wgpu::RenderPass) {
        // Reference: https://github.com/UnfoldedInc/deck.gl-native/blob/a8c4f6839c82221765dc7fa48f204e514060dcce/cpp/modules/luma.gl/core/src/model.cc#L171
        let mut location = 0;
        for attribute in &self.attribute_table.columns {
            pass.set_vertex_buffer(location, attribute.get_buffer().slice(..));
            location += 1;
        }
        for attribute in &self.instanced_attribute_table.columns {
            pass.set_vertex_buffer(location, attribute.get_buffer().slice(..));
            location += 1;
        }
    }
}

pub trait GetModel {
    /// Given a value from the domain, returns the corresponding value in the range.
    fn get_model(&self, device: &wgpu::Device) -> Model;
}
