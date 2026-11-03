mod engine;

pub use engine::{Engine, Wrapper, emitted, run};

pub fn side() -> u32 {
    engine::shapes::unit_side()
}
