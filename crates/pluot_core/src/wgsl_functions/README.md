WGSL functions that can be injected into shaders at either compile time or runtime by the ShaderBuilder.

If there are multiple variants of the same function name (with the same function signature), we store them with the convention: `wgsl_functions/{function_name}/{variant_name}.wgsl`
