//! Data models for observed `WordPress` REST API responses.
//!
//! These models cover a subset of core resources and plugin extensions. Resource models reject
//! unknown fields, and enums accept only their listed variants; responses from other configurations
//! may need additional model support.

#![allow(missing_docs)]

pub mod api;
