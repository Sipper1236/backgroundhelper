//! Pure job placement and fairness logic.

pub mod fair;
pub mod queue;
pub mod strategy;

pub use fair::*;
pub use queue::*;
pub use strategy::*;