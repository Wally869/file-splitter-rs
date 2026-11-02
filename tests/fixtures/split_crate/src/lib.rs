mod engine;

pub use engine::{Engine, Wrapper, run};

pub fn side() -> u32 {
    engine::shapes::unit_side()
}
