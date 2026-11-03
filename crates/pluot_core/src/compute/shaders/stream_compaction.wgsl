// Stable stream compaction of keyed elements into one contiguous run per
// stratum, in three passes over blocks of BLOCK elements:
//   1. `count_blocks` counts each block's elements per stratum, into
//      `block_offsets[stratum * num_blocks + block]`.
//   2. An exclusive scan of `block_offsets` (see exclusive_scan.wgsl) turns
//      those counts into where each block's run of each stratum starts, since
//      the layout orders runs by stratum, then by block.
//   3. `scatter` writes each element to its block's run, after the elements
//      of the same block and stratum that precede it.

const NO_STRATUM: u32 = 0xFFFFFFFFu;
const BLOCK: u32 = 256u;

struct Uniforms {
    num_elements: u32,
    num_blocks: u32,
    num_strata: u32,
    // Nonzero when element `i` refers to row `rows[i]`, rather than to row `i`.
    has_rows: u32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> keys: array<u32>;
@group(0) @binding(2) var<storage, read_write> block_offsets: array<u32>;
@group(0) @binding(3) var<storage, read> rows: array<u32>;
@group(0) @binding(4) var<storage, read_write> compacted: array<u32>;
@group(0) @binding(5) var<storage, read_write> mask_words: array<u32>;

var<workgroup> stratum_counts: array<atomic<u32>, {{max_strata}}>;
var<workgroup> block_keys: array<u32, BLOCK>;

fn block_index(workgroup_id: vec3<u32>, num_workgroups: vec3<u32>) -> u32 {
    return workgroup_id.x + workgroup_id.y * num_workgroups.x;
}

@compute @workgroup_size(256)
fn count_blocks(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(num_workgroups) num_workgroups: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let block = block_index(workgroup_id, num_workgroups);
    if (block >= uniforms.num_blocks) {
        return;
    }
    for (var stratum = local_index; stratum < uniforms.num_strata; stratum = stratum + BLOCK) {
        atomicStore(&stratum_counts[stratum], 0u);
    }
    workgroupBarrier();
    let i = block * BLOCK + local_index;
    if (i < uniforms.num_elements) {
        let key = keys[i];
        if (key != NO_STRATUM) {
            atomicAdd(&stratum_counts[key], 1u);
        }
    }
    workgroupBarrier();
    for (var stratum = local_index; stratum < uniforms.num_strata; stratum = stratum + BLOCK) {
        block_offsets[stratum * uniforms.num_blocks + block] = atomicLoad(&stratum_counts[stratum]);
    }
}

@compute @workgroup_size(256)
fn scatter(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(num_workgroups) num_workgroups: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let block = block_index(workgroup_id, num_workgroups);
    if (block >= uniforms.num_blocks) {
        return;
    }
    let i = block * BLOCK + local_index;
    var key = NO_STRATUM;
    if (i < uniforms.num_elements) {
        key = keys[i];
    }
    block_keys[local_index] = key;
    workgroupBarrier();
    if (key == NO_STRATUM) {
        return;
    }
    var rank = 0u;
    for (var j = 0u; j < local_index; j = j + 1u) {
        if (block_keys[j] == key) {
            rank = rank + 1u;
        }
    }
    var row = i;
    if (uniforms.has_rows != 0u) {
        row = rows[i];
    }
    compacted[block_offsets[key * uniforms.num_blocks + block] + rank] = row;
}

// Packs one byte per element (1 when it has a stratum, else 0) four to a
// word, so that the words read back as a little-endian `u8` mask.
@compute @workgroup_size(256)
fn pack_mask(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(num_workgroups) num_workgroups: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let word = block_index(workgroup_id, num_workgroups) * BLOCK + local_index;
    if (word * 4u >= uniforms.num_elements) {
        return;
    }
    var packed = 0u;
    for (var byte = 0u; byte < 4u; byte = byte + 1u) {
        let i = word * 4u + byte;
        if (i < uniforms.num_elements && keys[i] != NO_STRATUM) {
            packed = packed | (1u << (8u * byte));
        }
    }
    mask_words[word] = packed;
}
