// Types which represent the zarr attributes used to store anndata objects via zarr.
// This can be thought of as the AnnData analog of https://github.com/zarrs/ome_zarr_metadata (the latter of which is for OME-Zarr, as opposed to AnnData-Zarr).
// References:
// - https://github.com/scverse/anndata/blob/main/docs/fileformat-prose.md
// - https://github.com/scverse/anndata-rs
// - https://github.com/SingleRust/anndata-rs
// - https://github.com/ilan-gold/anndata.js/blob/main/src/io.ts
// - https://github.com/keller-mark/zod-anndata

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

/*
 * The `attributes` object of a zarr.json is always at least
 * { "encoding-type": ..., "encoding-version": ... }, sometimes with a few
 * extra fields depending on the encoding-type. Each encoding-type is pinned
 * to exactly one encoding-version, e.g.:
{
  "encoding-type": "anndata",
  "encoding-version": "0.1.0"
}
{
  "ordered": false,
  "encoding-type": "categorical",
  "encoding-version": "0.2.0"
}
 */

// serde has no literal-string type, so this generates a unit struct per
// literal whose (de)serialization only accepts that exact string, rejecting
// anything else during deserialization.
macro_rules! string_literal {
    ($name:ident, $value:literal) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name;

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str($value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = String::deserialize(deserializer)?;
                if s == $value {
                    Ok($name)
                } else {
                    Err(D::Error::custom(format!(
                        "expected literal \"{}\", got \"{}\"",
                        $value, s
                    )))
                }
            }
        }
    };
}

// Every encoding-type in the store uses one of these two encoding-versions.
string_literal!(V0_1_0, "0.1.0");
string_literal!(V0_2_0, "0.2.0");


// Note: This may need to change in order to support multiple encoding_version variants
// for the same encoding-type (see zod-anndata for examples).
// See the below string_enum for an example
// (would need to specify encoding_version: StringEncodingVersion,
// rather than encoding_version: V0_2_0).

/*

// For encoding-types that accept more than one encoding-version, generates an
// enum whose variants (de)serialize to fixed string literals, rejecting any
// value not in the list (see zod-anndata for examples of such encoding-types).
macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let s = match self {
                    $(Self::$variant => $value,)+
                };
                serializer.serialize_str(s)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = String::deserialize(deserializer)?;
                match s.as_str() {
                    $($value => Ok(Self::$variant),)+
                    other => Err(D::Error::custom(format!(
                        "expected one of [{}], got \"{}\"",
                        [$($value),+].join(", "),
                        other
                    ))),
                }
            }
        }
    };
}

// "string" scalars have been observed with both encoding-versions in the wild.
string_enum!(StringEncodingVersion {
    V0_1_0 => "0.1.0",
    V0_2_0 => "0.2.0",
});

*/

/// The `attributes` object of any group or array `zarr.json` within an
/// AnnData-Zarr store, discriminated by `encoding-type`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "encoding-type")]
pub enum AnnDataEncoding {
    #[serde(rename = "anndata")]
    AnnData {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    #[serde(rename = "raw")]
    Raw {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    #[serde(rename = "dict")]
    Dict {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    #[serde(rename = "nullable-integer")]
    NullableInteger {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    #[serde(rename = "nullable-boolean")]
    NullableBoolean {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    // `na-value` is optional per spec (defaults to "NA" semantics when absent).
    #[serde(rename = "nullable-string-array")]
    NullableStringArray {
        #[serde(rename = "na-value", default, skip_serializing_if = "Option::is_none")]
        na_value: Option<String>,
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    // Experimental as of anndata 0.9.x; `form` is a JSON-encoded awkward-array Form.
    #[serde(rename = "awkward-array")]
    AwkwardArray {
        form: String,
        length: u64,
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    #[serde(rename = "csr_matrix")]
    CsrMatrix {
        shape: Vec<u64>,
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    #[serde(rename = "csc_matrix")]
    CscMatrix {
        shape: Vec<u64>,
        #[serde(rename = "encoding-version")]
        encoding_version: V0_1_0,
    },
    #[serde(rename = "array")]
    Array {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_2_0,
    },
    #[serde(rename = "string-array")]
    StringArray {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_2_0,
    },
    #[serde(rename = "string")]
    String {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_2_0,
    },
    #[serde(rename = "numeric-scalar")]
    NumericScalar {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_2_0,
    },
    #[serde(rename = "rec-array")]
    RecArray {
        #[serde(rename = "encoding-version")]
        encoding_version: V0_2_0,
    },
    #[serde(rename = "categorical")]
    Categorical {
        ordered: bool,
        #[serde(rename = "encoding-version")]
        encoding_version: V0_2_0,
    },
    #[serde(rename = "dataframe")]
    DataFrame {
        #[serde(rename = "column-order")]
        column_order: Vec<String>,
        #[serde(rename = "_index")]
        index: String,
        #[serde(rename = "encoding-version")]
        encoding_version: V0_2_0,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real `attributes` objects pulled from data/out/pbmc68k.adata.zarr and
    // from the anndata fileformat-prose.md examples.
    #[test]
    fn deserializes_real_world_examples() {
        let cases = [
            (r#"{"encoding-type":"anndata","encoding-version":"0.1.0"}"#, AnnDataEncoding::AnnData { encoding_version: V0_1_0 }),
            (r#"{"encoding-type":"raw","encoding-version":"0.1.0"}"#, AnnDataEncoding::Raw { encoding_version: V0_1_0 }),
            (r#"{"encoding-type":"dict","encoding-version":"0.1.0"}"#, AnnDataEncoding::Dict { encoding_version: V0_1_0 }),
            (r#"{"encoding-type":"array","encoding-version":"0.2.0"}"#, AnnDataEncoding::Array { encoding_version: V0_2_0 }),
            (r#"{"encoding-type":"string-array","encoding-version":"0.2.0"}"#, AnnDataEncoding::StringArray { encoding_version: V0_2_0 }),
            (r#"{"encoding-type":"string","encoding-version":"0.2.0"}"#, AnnDataEncoding::String { encoding_version: V0_2_0 }),
            (r#"{"encoding-type":"numeric-scalar","encoding-version":"0.2.0"}"#, AnnDataEncoding::NumericScalar { encoding_version: V0_2_0 }),
            (r#"{"encoding-type":"rec-array","encoding-version":"0.2.0"}"#, AnnDataEncoding::RecArray { encoding_version: V0_2_0 }),
            (
                r#"{"ordered":false,"encoding-type":"categorical","encoding-version":"0.2.0"}"#,
                AnnDataEncoding::Categorical { ordered: false, encoding_version: V0_2_0 },
            ),
            (
                r#"{"na-value":"NaN","encoding-type":"nullable-string-array","encoding-version":"0.1.0"}"#,
                AnnDataEncoding::NullableStringArray { na_value: Some("NaN".to_string()), encoding_version: V0_1_0 },
            ),
            (
                r#"{"column-order":["a","b"],"_index":"index","encoding-type":"dataframe","encoding-version":"0.2.0"}"#,
                AnnDataEncoding::DataFrame {
                    column_order: vec!["a".to_string(), "b".to_string()],
                    index: "index".to_string(),
                    encoding_version: V0_2_0,
                },
            ),
            (
                r#"{"shape":[700,700],"encoding-type":"csr_matrix","encoding-version":"0.1.0"}"#,
                AnnDataEncoding::CsrMatrix { shape: vec![700, 700], encoding_version: V0_1_0 },
            ),
        ];

        for (json, expected) in cases {
            let parsed: AnnDataEncoding = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, expected, "mismatch deserializing {json}");
        }
    }

    #[test]
    fn nullable_string_array_na_value_is_optional() {
        let json = r#"{"encoding-type":"nullable-string-array","encoding-version":"0.1.0"}"#;
        let parsed: AnnDataEncoding = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed,
            AnnDataEncoding::NullableStringArray { na_value: None, encoding_version: V0_1_0 }
        );
    }

    #[test]
    fn awkward_array_round_trips() {
        let original = AnnDataEncoding::AwkwardArray {
            form: r#"{"class":"RecordArray"}"#.to_string(),
            length: 40145,
            encoding_version: V0_1_0,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: AnnDataEncoding = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn rejects_mismatched_encoding_version() {
        // "array" is pinned to "0.2.0"; supplying "0.1.0" must fail, not silently coerce.
        let json = r#"{"encoding-type":"array","encoding-version":"0.1.0"}"#;
        let err = serde_json::from_str::<AnnDataEncoding>(json).unwrap_err();
        assert!(err.to_string().contains("0.1.0"));
    }

    #[test]
    fn rejects_unknown_encoding_type() {
        let json = r#"{"encoding-type":"not-a-real-type","encoding-version":"0.1.0"}"#;
        assert!(serde_json::from_str::<AnnDataEncoding>(json).is_err());
    }

    #[test]
    fn serializes_back_to_expected_shape() {
        let value = AnnDataEncoding::Categorical { ordered: true, encoding_version: V0_2_0 };
        let json: serde_json::Value = serde_json::to_value(&value).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "encoding-type": "categorical",
                "ordered": true,
                "encoding-version": "0.2.0",
            })
        );
    }

    #[test]
    fn string_literal_marker_rejects_wrong_value() {
        let err = serde_json::from_str::<V0_1_0>(r#""0.2.0""#).unwrap_err();
        assert!(err.to_string().contains("0.1.0"));
        assert!(serde_json::from_str::<V0_1_0>(r#""0.1.0""#).is_ok());
    }
}
