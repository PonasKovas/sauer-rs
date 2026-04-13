//! `ogz-rs` is a strictly-typed parser and serializer for the Cube 2: Sauerbraten `.ogz` map format.
//! It seamlessly handles all map versions up to 33, resolving all engine quirks including the historic
//! broken GZIP trailer issue via lenient decoding.

pub mod data;
mod de;
mod error;
mod ser;

pub use data::OgzMap;
pub use error::OgzError;
