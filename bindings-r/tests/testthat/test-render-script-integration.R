normalize_svg <- function(svg) {
  # Trim each line and drop blanks, mirroring the Rust `check_svg_snapshot`
  # normalization in crates/pluot/tests/test_utils/snapshot_utils.rs.
  lines <- trimws(strsplit(svg, "\n", fixed = TRUE)[[1]])
  paste(lines[nzchar(lines)], collapse = "\n")
}

test_that("executing the generated render_script.R reproduces the canonical SVG", {
  fixtures_dir <- Sys.getenv("PLUOT_RENDER_SCRIPT_FIXTURES_DIR")
  skip_if(
    identical(fixtures_dir, ""),
    "PLUOT_RENDER_SCRIPT_FIXTURES_DIR not set; run scripts/test_r_render_script_integration.sh instead of R CMD check directly"
  )

  script_path <- file.path(fixtures_dir, "render_script.R")
  canonical_path <- file.path(fixtures_dir, "canonical.svg")

  script_env <- new.env(parent = globalenv())
  sys.source(script_path, envir = script_env)
  img <- script_env$img

  canonical_svg <- paste(readLines(canonical_path), collapse = "\n")

  expect_equal(normalize_svg(img), normalize_svg(canonical_svg))
})
