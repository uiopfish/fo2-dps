pub mod character;
pub mod db;
pub mod effects;
pub mod encounter;
pub mod grinding;
pub mod mechanics;
pub mod mobs;
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
pub mod pages;
#[cfg(not(target_arch = "wasm32"))]
pub mod scraper;
#[cfg(not(target_arch = "wasm32"))]
pub mod storage;
#[cfg(not(target_arch = "wasm32"))]
pub mod validation;

#[cfg(target_arch = "wasm32")]
mod wasm;
