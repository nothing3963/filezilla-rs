//! Error types live next to the shared DTOs; this module keeps the
//! `crate::error::*` import path stable across the backend.

pub use crate::models::{AppError, AppResult};
