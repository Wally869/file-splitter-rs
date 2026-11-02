mod engine;

pub use engine::{Engine, run};

pub fn side() -> u32 {
    engine::shapes::unit_side()
}
