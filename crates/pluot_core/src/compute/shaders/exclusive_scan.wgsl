// One level of an exclusive prefix sum: `scan_blocks` scans each block of
// BLOCK elements in place and writes each block's total to `block_sums`. When
// there is more than one block, the caller scans `block_sums` the same way,
// then `add_block_offsets` adds each block's scanned total to its elements.

const BLOCK: u32 = 256u;

struct Uniforms {
    len: u32,
    num_blocks: u32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read_write> data: array<u32>;
@group(0) @binding(2) var<storage, read_write> block_sums: array<u32>;

var<workgroup> partial_sums: array<u32, BLOCK>;

fn block_index(workgroup_id: vec3<u32>, num_workgroups: vec3<u32>) -> u32 {
    return workgroup_id.x + workgroup_id.y * num_workgroups.x;
}

@compute @workgroup_size(256)
fn scan_blocks(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(num_workgroups) num_workgroups: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let block = block_index(workgroup_id, num_workgroups);
    if (block >= uniforms.num_blocks) {
        return;
    }
    let i = block * BLOCK + local_index;
    var value = 0u;
    if (i < uniforms.len) {
        value = data[i];
    }
    partial_sums[local_index] = value;
    workgroupBarrier();
    // Hillis-Steele inclusive scan within the block.
    for (var offset = 1u; offset < BLOCK; offset = offset * 2u) {
        var addend = 0u;
        if (local_index >= offset) {
            addend = partial_sums[local_index - offset];
        }
        workgroupBarrier();
        partial_sums[local_index] = partial_sums[local_index] + addend;
        workgroupBarrier();
    }
    if (i < uniforms.len) {
        data[i] = partial_sums[local_index] - value;
    }
    if (local_index == BLOCK - 1u) {
        block_sums[block] = partial_sums[local_index];
    }
}

@compute @workgroup_size(256)
fn add_block_offsets(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(num_workgroups) num_workgroups: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let block = block_index(workgroup_id, num_workgroups);
    if (block >= uniforms.num_blocks) {
        return;
    }
    let i = block * BLOCK + local_index;
    if (i < uniforms.len) {
        data[i] = data[i] + block_sums[block];
    }
}
