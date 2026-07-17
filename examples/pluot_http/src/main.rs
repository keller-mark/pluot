//! An HTTP server for rendering plots to SVG or PNG.
//!
//! POST /render-svg - accepts JSON render params, returns SVG (image/svg+xml)
//! POST /render-png - accepts JSON render params, returns PNG (image/png)
//!
//! The request body is a JSON object whose fields correspond to [`pluot::RenderParams`].
//! Only `layers` is required; all other fields fall back to their defaults when omitted.
//! The `format` field is always overridden by the chosen endpoint.

use std::pin::Pin;

use gotham::handler::{HandlerError, HandlerFuture};
use gotham::helpers::http::response::create_response;
use gotham::helpers::http::Body;
use gotham::http::StatusCode;
use gotham::http::Method;
use gotham::http_body_util::BodyExt;
use gotham::prelude::*;
use gotham::router::{build_simple_router, Router};
use gotham::state::State;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use pluot::{render, GraphicsFormat, RenderParams};

/// Shared implementation for both render endpoints.
/// Reads the JSON body, merges missing fields with [`RenderParams`] defaults,
/// calls [`render`], then returns the encoded output with the appropriate MIME type.
fn do_render(mut state: State, format: GraphicsFormat) -> Pin<Box<HandlerFuture>> {
    Box::pin(async move {
        // Collect the request body bytes.
        let body = Body::take_from(&mut state);
        let bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return Err((state, HandlerError::from(e).with_status(StatusCode::BAD_REQUEST)));
            }
        };

        // Parse the body as a JSON object.
        let mut body_value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return Err((state, HandlerError::from(e).with_status(StatusCode::BAD_REQUEST)));
            }
        };

        // Fill in any missing fields using RenderParams defaults so that the
        // caller only needs to supply the fields they care about.
        let defaults = serde_json::to_value(RenderParams::default()).unwrap();
        if let (Some(body_obj), Some(defaults_obj)) =
            (body_value.as_object_mut(), defaults.as_object())
        {
            for (key, val) in defaults_obj {
                body_obj.entry(key).or_insert_with(|| val.clone());
            }
        }

        // Deserialize into RenderParams.
        let mut params: RenderParams = match serde_json::from_value(body_value) {
            Ok(p) => p,
            Err(e) => {
                return Err((state, HandlerError::from(e).with_status(StatusCode::BAD_REQUEST)));
            }
        };

        // Override fields that the endpoint controls or that make no sense for
        // static (non-interactive) HTTP rendering.
        params.format = format;
        params.timeout = None;
        params.cache_enabled = false;
        params.svg_compression_enabled = false;
        params.svg_include_document = true;
        params.pickable = false;

        let width = params.width;
        let height = params.height;
        let is_vector = params.format == GraphicsFormat::Vector;

        let result = render(params).await;

        if is_vector {
            let svg_mime: gotham::mime::Mime = "image/svg+xml".parse().unwrap();
            let response = create_response(&state, StatusCode::OK, svg_mime, result);
            Ok((state, response))
        } else {
            // The raster render returns raw RGBA pixels followed by 1 extra byte
            // (the bailed_early flag). Strip it before encoding.
            let pixel_data = &result[..result.len() - 1];

            let mut png_bytes: Vec<u8> = Vec::new();
            let encoder = PngEncoder::new(&mut png_bytes);
            match encoder.write_image(pixel_data, width, height, ExtendedColorType::Rgba8) {
                Ok(_) => {}
                Err(e) => {
                    return Err((
                        state,
                        HandlerError::from(e).with_status(StatusCode::INTERNAL_SERVER_ERROR),
                    ));
                }
            }

            let response =
                create_response(&state, StatusCode::OK, gotham::mime::IMAGE_PNG, png_bytes);
            Ok((state, response))
        }
    })
}

fn render_svg(state: State) -> Pin<Box<HandlerFuture>> {
    do_render(state, GraphicsFormat::Vector)
}

fn render_png(state: State) -> Pin<Box<HandlerFuture>> {
    do_render(state, GraphicsFormat::Raster)
}

/// Create a `Handler` that is invoked for requests to the path "/"
pub fn say_hello(state: State) -> (State, &'static str) {
    (state, "pluot HTTP renderer: POST /render-svg or /render-png with JSON body")
}

fn router() -> Router {
    build_simple_router(|route| {
        route
            .request(vec![Method::GET, Method::HEAD], "/")
            .to(say_hello);

        route.post("/render-svg").to(render_svg);
        route.post("/render-png").to(render_png);
    })
}

pub fn main() {
    let addr = "127.0.0.1:7878";
    println!("Listening for requests at http://{}", addr);
    gotham::start(addr, router()).unwrap();
}
