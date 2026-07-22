//! "Rendering to code": rather than rasterizing to pixels or an SVG, turn a
//! [`RenderParams`] into a string of source code (or JSON) that reproduces the
//! same plot when run against one of the language bindings.
//!
//! Each generator serializes the params to a `serde_json::Value` once and then
//! walks that value, emitting language-specific literal syntax. Two flavors are
//! produced per language:
//!
//! - **`Expression*`** — a single expression (a function call, or a JSX element)
//!   with no imports or surrounding statements, suitable for embedding.
//! - **`Script*`** — a self-contained script including the imports, variable
//!   definitions and library initialization needed to run standalone.
//!
//! Targets:
//!
//! - [`GraphicsFormat::Json`]: the params as pretty-printed JSON (the wire
//!   format accepted by every binding's `render` entry point).
//! - Python (`bindings-python`): a `pluot.render_to_image(...)` call.
//! - R (`bindings-r`): a `render_to_raster(...)` call.
//! - JS (`bindings-js`): a `render_wasm(...)` call.
//! - JSX / React (`@pluot/react`): a `<Pluot />` element / component.
//! - HTML: a standalone page that loads `@pluot/core` and renders to a canvas.
//! - Rust: a `pluot_core::render(...)` call.

use crate::params::{GraphicsFormat, RenderParams};
use serde_json::Value;

/// Serialize `params` into code (source or JSON) in the language and flavor
/// implied by `format`.
///
/// Panics if `format` is not a code format (see [`GraphicsFormat::is_code`]).
pub fn render_to_script(params: &RenderParams, format: &GraphicsFormat) -> String {
    // Serialize once; every generator walks this JSON value.
    let value = serde_json::to_value(params).expect("RenderParams should serialize to JSON");

    match format {
        GraphicsFormat::Json => to_json(&value),

        GraphicsFormat::ExpressionPython => format!("{}\n", python_call(&value)),
        GraphicsFormat::ScriptPython => python_script(&value),

        GraphicsFormat::ExpressionR => format!("{}\n", r_call(&value)),
        GraphicsFormat::ScriptR => r_script(&value),

        GraphicsFormat::ExpressionJs => format!("{}\n", js_call(&value)),
        GraphicsFormat::ScriptJs => js_script(&value),

        GraphicsFormat::ExpressionJsx => format!("{}\n", jsx_element(&value, 0)),
        GraphicsFormat::ScriptReact => react_script(&value),
        GraphicsFormat::ScriptHtml => html_script(&value),

        GraphicsFormat::ExpressionRust => format!("{}\n", rust_expr(&value)),
        GraphicsFormat::ScriptRust => rust_script(&value),

        other => panic!("render_to_script called with a non-code format: {other:?}"),
    }
}

/// Return a clone of `value` with its `format` field forced to `"Raster"`.
///
/// The serialized params carry the `Expression*`/`Script*` format that requested
/// code generation, which would be nonsensical (and circular) inside the emitted
/// code. Generated code describes how to produce the *plot*, so it defaults to
/// raster output.
fn with_format_raster(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(obj) = value.as_object_mut() {
        obj.insert("format".to_string(), Value::String("Raster".to_string()));
    }
    value
}

/// A JSON-escaped, double-quoted string literal. JSON string escaping (`\"`,
/// `\\`, `\n`, `\uXXXX`, …) is a valid subset of the string syntax of Python,
/// JavaScript and R, so this is reused across those generators.
fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("string should serialize to JSON")
}

// === JSON ===

fn to_json(value: &Value) -> String {
    serde_json::to_string_pretty(&with_format_raster(value))
        .expect("RenderParams JSON should pretty-print")
}

// === Generic curly-brace literal emitter (Python / JavaScript) ===

/// Per-language tokens for the curly-brace literal emitter shared by Python and
/// JavaScript (and the object/array subtrees embedded in JSX props / HTML).
struct CurlySyntax {
    /// One indentation level.
    indent: &'static str,
    null: &'static str,
    true_: &'static str,
    false_: &'static str,
    /// When `true`, object keys are emitted as quoted strings (Python dict
    /// literals). When `false`, bare identifiers are used where valid, quoting
    /// only keys that are not valid identifiers (JavaScript object literals).
    quote_keys: bool,
}

const PYTHON_SYNTAX: CurlySyntax = CurlySyntax {
    indent: "    ",
    null: "None",
    true_: "True",
    false_: "False",
    quote_keys: true,
};

const JS_SYNTAX: CurlySyntax = CurlySyntax {
    indent: "  ",
    null: "null",
    true_: "true",
    false_: "false",
    quote_keys: false,
};

/// Whether `s` is a valid identifier in Python/JavaScript (ASCII subset), so it
/// can be used as an unquoted object key.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Emit `value` as a curly-brace-language literal (dict/object + list/array),
/// indented so that the opening token sits at the current column and the closing
/// token aligns to `level`.
fn emit_curly(value: &Value, level: usize, syn: &CurlySyntax) -> String {
    match value {
        Value::Null => syn.null.to_string(),
        Value::Bool(true) => syn.true_.to_string(),
        Value::Bool(false) => syn.false_.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => quoted(s),
        Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".to_string();
            }
            let inner = syn.indent.repeat(level + 1);
            let close = syn.indent.repeat(level);
            let items: Vec<String> = arr
                .iter()
                .map(|item| format!("{inner}{}", emit_curly(item, level + 1, syn)))
                .collect();
            format!("[\n{}\n{close}]", items.join(",\n"))
        }
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }
            let inner = syn.indent.repeat(level + 1);
            let close = syn.indent.repeat(level);
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let key = if syn.quote_keys || !is_identifier(k) {
                        quoted(k)
                    } else {
                        k.clone()
                    };
                    format!("{inner}{key}: {}", emit_curly(v, level + 1, syn))
                })
                .collect();
            format!("{{\n{}\n{close}}}", items.join(",\n"))
        }
    }
}

// === Python ===

/// The `render_to_image(...)` call expression. `render_to_image` forces raster
/// output, so `format` is omitted; every other top-level field maps directly to
/// a keyword argument (`plot_type` and `plot_params` are already separate keys
/// thanks to the flattened enum).
fn python_call(value: &Value) -> String {
    let obj = value.as_object().expect("RenderParams serializes to an object");
    let args: Vec<String> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "format")
        .map(|(k, v)| format!("    {k}={},", emit_curly(v, 1, &PYTHON_SYNTAX)))
        .collect();
    format!("render_to_image(\n{}\n)", args.join("\n"))
}

fn python_script(value: &Value) -> String {
    // PEP 723 inline script metadata so the file is runnable via e.g. `uv run`.
    format!(
        "# /// script\n\
         # requires-python = \">=3.9\"\n\
         # dependencies = [\n\
         #     \"pluot\",\n\
         # ]\n\
         # ///\n\
         from pluot import render_to_image\n\
         \n\
         # Zarr store(s) are declared in the `stores` map below and constructed\n\
         # from their metadata; pass `store=`/`stores=` to override with your own\n\
         # store object(s).\n\
         img = await {}\n",
        python_call(value),
    )
}

// === R ===

/// Emit `value` as an R literal. Objects become named `list(...)`s; arrays of
/// scalars become `c(...)` vectors while arrays containing objects/arrays become
/// `list(...)`s.
fn emit_r(value: &Value, level: usize) -> String {
    const INDENT: &str = "  ";
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(true) => "TRUE".to_string(),
        Value::Bool(false) => "FALSE".to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => quoted(s),
        Value::Array(arr) => {
            if arr.is_empty() {
                return "list()".to_string();
            }
            // A flat vector of scalars maps most naturally onto an R atomic
            // vector; anything with nested structure needs a list.
            let all_scalars = arr
                .iter()
                .all(|v| matches!(v, Value::Number(_) | Value::String(_) | Value::Bool(_)));
            let ctor = if all_scalars { "c" } else { "list" };
            let inner = INDENT.repeat(level + 1);
            let close = INDENT.repeat(level);
            let items: Vec<String> = arr
                .iter()
                .map(|item| format!("{inner}{}", emit_r(item, level + 1)))
                .collect();
            format!("{ctor}(\n{}\n{close})", items.join(",\n"))
        }
        Value::Object(map) => {
            if map.is_empty() {
                return "list()".to_string();
            }
            let inner = INDENT.repeat(level + 1);
            let close = INDENT.repeat(level);
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{inner}{k} = {}", emit_r(v, level + 1)))
                .collect();
            format!("list(\n{}\n{close})", items.join(",\n"))
        }
    }
}

/// The `render_to_raster(...)` call expression. The R API takes the layer list
/// directly (rather than a nested plot_type/plot_params object) and
/// `render_to_raster` forces raster output.
fn r_call(value: &Value) -> String {
    let obj = value.as_object().expect("RenderParams serializes to an object");

    let mut args: Vec<String> = Vec::new();

    let layers = obj
        .get("plot_params")
        .and_then(|p| p.get("layers"))
        .cloned()
        .unwrap_or_else(|| Value::Array(vec![]));
    args.push(format!("  layers = {}", emit_r(&layers, 1)));

    for (k, v) in obj {
        if matches!(k.as_str(), "format" | "plot_type" | "plot_params") {
            continue;
        }
        args.push(format!("  {k} = {}", emit_r(v, 1)));
    }

    format!("render_to_raster(\n{}\n)", args.join(",\n"))
}

fn r_script(value: &Value) -> String {
    format!(
        "library(pluotr)\n\
         \n\
         # Zarr store(s) are declared in the `stores` list below and constructed\n\
         # from their metadata; use `pluot_register_store()` to override with your\n\
         # own store object(s).\n\
         img <- {}\n",
        r_call(value),
    )
}

// === JavaScript ===

/// The `render_wasm({...})` call expression, with the params object inlined.
fn js_call(value: &Value) -> String {
    format!(
        "render_wasm({})",
        emit_curly(&with_format_raster(value), 0, &JS_SYNTAX)
    )
}

fn js_script(value: &Value) -> String {
    let params = emit_curly(&with_format_raster(value), 0, &JS_SYNTAX);
    format!(
        "import {{ initialize, render_wasm, setStoreByName }} from \"@pluot/core\";\n\
         \n\
         await initialize();\n\
         // Zarr store(s) are declared in the `stores` map below and constructed\n\
         // from their metadata; call `setStoreByName(\"my_store\", store)` before\n\
         // rendering to override with your own store object.\n\
         \n\
         const renderParams = {params};\n\
         \n\
         // Returns a Uint8Array of RGBA bytes (plus one trailing status byte).\n\
         const result = await render_wasm(renderParams);\n",
    )
}

// === JSX / React ===

/// Map a top-level `RenderParams` (snake_case) key to the corresponding camelCase
/// `<Pluot />` prop name, or `None` if the component does not expose that param.
fn jsx_prop_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "width" => "width",
        "height" => "height",
        "plot_id" => "plotId",
        "plot_type" => "plotType",
        "stores" => "stores",
        "plot_params" => "plotParams",
        "view_mode" => "viewMode",
        "camera_view" => "cameraMatrix",
        "aspect_ratio_mode" => "aspectRatioMode",
        "aspect_ratio_alignment_mode" => "aspectRatioAlignmentMode",
        "margin_left" => "marginLeft",
        "margin_right" => "marginRight",
        "margin_top" => "marginTop",
        "margin_bottom" => "marginBottom",
        "pickable" => "enablePicking",
        "format" => "format",
        // Props not exposed by the <Pluot /> component (device_pixel_ratio,
        // timeout, cache_enabled, svg_*, wait_for_store_gets, render_backend,
        // compute_backend) are skipped.
        _ => return None,
    })
}

/// A `<Pluot ... />` element, with the `<Pluot` / `/>` lines indented `base`
/// levels (two spaces per level) and props one level deeper.
fn jsx_element(value: &Value, base: usize) -> String {
    let obj = value.as_object().expect("RenderParams serializes to an object");
    let pad = "  ".repeat(base);
    let prop_pad = "  ".repeat(base + 1);

    let mut props: Vec<String> = Vec::new();
    for (k, v) in obj {
        let Some(name) = jsx_prop_name(k) else {
            continue;
        };
        if k == "format" {
            // Default the component to raster output regardless of the requested
            // code format.
            props.push(format!("{prop_pad}format=\"Raster\""));
            continue;
        }
        // The component supplies its own defaults, so drop absent optional
        // values (e.g. a null camera matrix or unset margins).
        if v.is_null() {
            continue;
        }
        // JSX: string props use `name="..."`; everything else is a `{expr}`.
        let rendered = match v {
            Value::String(s) => format!("{prop_pad}{name}={}", quoted(s)),
            _ => format!("{prop_pad}{name}={{{}}}", emit_curly(v, base + 1, &JS_SYNTAX)),
        };
        props.push(rendered);
    }

    format!("{pad}<Pluot\n{}\n{pad}/>", props.join("\n"))
}

fn react_script(value: &Value) -> String {
    // The element is nested inside `return ( ... )` in the component body.
    let element = jsx_element(value, 2);
    format!(
        "import React from \"react\";\n\
         import {{ Pluot }} from \"@pluot/react\";\n\
         \n\
         // Zarr store(s) are declared via the `stores` prop and constructed from\n\
         // their metadata; pass a `store` prop to override with your own object.\n\
         export function PluotPlot() {{\n\
         \x20 return (\n\
         {element}\n\
         \x20 );\n\
         }}\n",
    )
}

// === HTML ===

fn html_script(value: &Value) -> String {
    let obj = value.as_object().expect("RenderParams serializes to an object");
    let width = obj.get("width").and_then(Value::as_u64).unwrap_or(0);
    let height = obj.get("height").and_then(Value::as_u64).unwrap_or(0);
    // Indent the params object to sit under the module script (6 spaces).
    let params = emit_curly(&with_format_raster(value), 3, &JS_SYNTAX);

    format!(
        "<!DOCTYPE html>\n\
         <html lang=\"en\">\n\
         \x20 <head>\n\
         \x20   <meta charset=\"utf-8\" />\n\
         \x20   <title>Pluot plot</title>\n\
         \x20 </head>\n\
         \x20 <body>\n\
         \x20   <canvas id=\"pluot-canvas\" width=\"{width}\" height=\"{height}\"></canvas>\n\
         \x20   <script type=\"module\">\n\
         \x20     import {{ initialize, render_wasm, setStoreByName }} from \"https://esm.sh/@pluot/core\";\n\
         \n\
         \x20     await initialize();\n\
         \x20     // Zarr store(s) are declared in the `stores` map below and built from\n\
         \x20     // their metadata; call `setStoreByName(\"my_store\", store)` to override.\n\
         \n\
         \x20     const renderParams = {params};\n\
         \n\
         \x20     const result = await render_wasm(renderParams);\n\
         \n\
         \x20     // Draw the RGBA bytes (minus the trailing status byte) to the canvas.\n\
         \x20     const canvas = document.getElementById(\"pluot-canvas\");\n\
         \x20     const ctx = canvas.getContext(\"2d\");\n\
         \x20     const imageData = new ImageData(\n\
         \x20       new Uint8ClampedArray(result.subarray(0, -1)),\n\
         \x20       renderParams.width,\n\
         \x20       renderParams.height,\n\
         \x20     );\n\
         \x20     ctx.putImageData(imageData, 0, 0);\n\
         \x20   </script>\n\
         \x20 </body>\n\
         </html>\n",
    )
}

// === Rust ===

/// Wrap `content` in a Rust raw string literal using enough `#` delimiters that
/// the terminator cannot appear inside the content.
fn rust_raw_string(content: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '#' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let hashes = "#".repeat(longest + 1);
    format!("r{hashes}\"{content}\"{hashes}")
}

/// A single `render(...)` expression that deserializes compact JSON at runtime.
/// Reconstructing the fully-typed `RenderParams` struct literal (nested enums,
/// `Option`s, layer params) would be far more brittle than round-tripping
/// through JSON.
fn rust_expr(value: &Value) -> String {
    let json = serde_json::to_string(&with_format_raster(value))
        .expect("RenderParams JSON should serialize");
    format!(
        "pluot_core::render(serde_json::from_str::<pluot_core::RenderParams>({}).unwrap())",
        rust_raw_string(&json),
    )
}

fn rust_script(value: &Value) -> String {
    let json = serde_json::to_string_pretty(&with_format_raster(value))
        .expect("RenderParams JSON should pretty-print");
    format!(
        "use pluot_core::{{render, RenderParams}};\n\
         \n\
         // The plot parameters, as JSON.\n\
         let params_json = {};\n\
         let params: RenderParams =\n\
         \x20   serde_json::from_str(params_json).expect(\"valid RenderParams JSON\");\n\
         \n\
         // `render` is async; `.await` it inside an async runtime.\n\
         // Returns a Vec<u8> of RGBA bytes (plus one trailing status byte).\n\
         let pixels = render(params).await;\n",
        rust_raw_string(&json),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::RenderParams;

    fn sample_params(format: GraphicsFormat) -> RenderParams {
        let layers = serde_json::json!([
            {
                "layer_type": "PointLayer",
                "layer_params": {
                    "layer_id": "pts",
                    "position_x": { "dtype": "Float32", "values": [1.0, 2.0] },
                    "position_y": { "dtype": "Float32", "values": [3.0, 4.0] }
                }
            }
        ]);
        let stores = std::collections::HashMap::from([(
            "my_store".to_string(),
            crate::params::ZarrStoreInfo {
                store_params: crate::params::ZarrStoreParams::HttpStore(
                    crate::params::HttpStoreParams {
                        url: "https://example.com/my_store.zarr".to_string(),
                        options: None,
                    },
                ),
                store_extensions: None,
            },
        )]);
        RenderParams {
            width: 640,
            height: 480,
            format,
            stores: Some(stores),
            plot_id: "plot_1".to_string(),
            plot_params: serde_json::from_value(serde_json::json!({ "layers": layers }))
                .map(crate::params::PlotParams::LayeredPlot)
                .unwrap(),
            ..Default::default()
        }
    }

    #[test]
    fn json_is_valid_and_format_reset() {
        let params = sample_params(GraphicsFormat::Json);
        let out = render_to_script(&params, &GraphicsFormat::Json);
        let parsed: Value = serde_json::from_str(&out).expect("output should be valid JSON");
        assert_eq!(parsed["format"], Value::String("Raster".to_string()));
        assert_eq!(parsed["width"], Value::Number(640.into()));
        assert_eq!(parsed["plot_type"], Value::String("LayeredPlot".to_string()));
    }

    #[test]
    fn stores_round_trip_through_json() {
        // The top-level `stores` map uses a flattened, adjacently-tagged
        // ZarrStoreParams enum; make sure it survives a serialize -> JSON ->
        // deserialize round trip (and appears in the emitted JSON).
        let params = sample_params(GraphicsFormat::Json);
        let out = render_to_script(&params, &GraphicsFormat::Json);

        let parsed: Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(
            parsed["stores"]["my_store"]["store_type"],
            Value::String("HttpStore".to_string())
        );
        assert_eq!(
            parsed["stores"]["my_store"]["store_params"]["url"],
            Value::String("https://example.com/my_store.zarr".to_string())
        );

        let round_tripped: RenderParams =
            serde_json::from_str(&out).expect("stores should deserialize back into RenderParams");
        let stores = round_tripped.stores.expect("stores present");
        assert!(stores.contains_key("my_store"));
    }

    #[test]
    fn python_expression_is_a_bare_call() {
        let params = sample_params(GraphicsFormat::ExpressionPython);
        let out = render_to_script(&params, &GraphicsFormat::ExpressionPython);
        assert!(out.starts_with("render_to_image("));
        // An expression carries no imports.
        assert!(!out.contains("from pluot import"));
        assert!(out.contains("width=640"));
        assert!(!out.contains("format="));
    }

    #[test]
    fn python_script_has_imports() {
        let params = sample_params(GraphicsFormat::ScriptPython);
        let out = render_to_script(&params, &GraphicsFormat::ScriptPython);
        assert!(out.contains("from pluot import render_to_image"));
        assert!(out.contains("img = await render_to_image("));
    }

    #[test]
    fn r_expression_and_script() {
        let expr = render_to_script(
            &sample_params(GraphicsFormat::ExpressionR),
            &GraphicsFormat::ExpressionR,
        );
        assert!(expr.starts_with("render_to_raster("));
        assert!(expr.contains("layers = list("));
        assert!(!expr.contains("library(pluotr)"));

        let script = render_to_script(
            &sample_params(GraphicsFormat::ScriptR),
            &GraphicsFormat::ScriptR,
        );
        assert!(script.contains("library(pluotr)"));
        assert!(script.contains("img <- render_to_raster("));
    }

    #[test]
    fn js_expression_and_script() {
        let expr = render_to_script(
            &sample_params(GraphicsFormat::ExpressionJs),
            &GraphicsFormat::ExpressionJs,
        );
        assert!(expr.starts_with("render_wasm({"));
        assert!(!expr.contains("import"));

        let script = render_to_script(
            &sample_params(GraphicsFormat::ScriptJs),
            &GraphicsFormat::ScriptJs,
        );
        assert!(script.contains("from \"@pluot/core\""));
        assert!(script.contains("const renderParams = {"));
        assert!(script.contains("await render_wasm(renderParams)"));
    }

    #[test]
    fn jsx_expression_is_single_element() {
        let out = render_to_script(
            &sample_params(GraphicsFormat::ExpressionJsx),
            &GraphicsFormat::ExpressionJsx,
        );
        assert!(out.starts_with("<Pluot"));
        assert!(out.contains("width={640}"));
        assert!(out.contains("plotId=\"plot_1\""));
        assert!(out.contains("format=\"Raster\""));
        assert!(!out.contains("import"));
        assert!(!out.contains("plot_id="));
    }

    #[test]
    fn react_script_defines_component() {
        let out = render_to_script(
            &sample_params(GraphicsFormat::ScriptReact),
            &GraphicsFormat::ScriptReact,
        );
        assert!(out.contains("import { Pluot } from \"@pluot/react\""));
        assert!(out.contains("export function PluotPlot()"));
        assert!(out.contains("<Pluot"));
    }

    #[test]
    fn html_script_is_a_page() {
        let out = render_to_script(
            &sample_params(GraphicsFormat::ScriptHtml),
            &GraphicsFormat::ScriptHtml,
        );
        assert!(out.starts_with("<!DOCTYPE html>"));
        assert!(out.contains("<canvas id=\"pluot-canvas\" width=\"640\" height=\"480\">"));
        assert!(out.contains("render_wasm(renderParams)"));
        assert!(out.contains("esm.sh/@pluot/core"));
    }

    #[test]
    fn rust_expression_and_script() {
        let expr = render_to_script(
            &sample_params(GraphicsFormat::ExpressionRust),
            &GraphicsFormat::ExpressionRust,
        );
        assert!(expr.starts_with("pluot_core::render("));
        assert!(expr.contains("serde_json::from_str::<pluot_core::RenderParams>"));
        assert!(!expr.contains("use pluot_core"));

        let script = render_to_script(
            &sample_params(GraphicsFormat::ScriptRust),
            &GraphicsFormat::ScriptRust,
        );
        assert!(script.contains("use pluot_core::{render, RenderParams}"));
        assert!(script.contains("render(params).await"));
    }
}
