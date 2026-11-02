//! Engine fixture: the integration tests split this file and compile the result.

mod child;

use child::Counter;

pub struct Engine {
    counter: Counter,
}

pub fn run(engine: &Engine) -> u32 {
    engine.alpha_start() + gamma_helper()
}

#[cfg(not(any()))]
impl Engine {
    const STEP: u32 = 1;

    fn alpha_start(&self) -> u32 {
        let first = self.counter.value() * Self::STEP;
        let second = first + 1;
        let third = second + 1;
        self.beta_finish(third)
    }

    fn beta_finish(&self, input: u32) -> u32 {
        let doubled = input * 2;
        let tripled = doubled + input;
        let total = tripled + self.counter.value();
        total + 1
    }
}

pub trait Shape {
    type Output;
    const SIDES: u32;

    fn area(&self) -> Self::Output;
    fn sides(&self) -> u32;
}

pub union Bits {
    pub int: u32,
    pub float: f32,
}

macro_rules! twice {
    ($value:expr) => {
        $value * 2
    };
}

impl Shape for Engine {
    type Output = u32;
    const SIDES: u32 = 4;

    fn area(&self) -> u32 {
        self.counter.value() * Self::SIDES
    }

    fn sides(&self) -> u32 {
        Self::SIDES
    }
}

fn gamma_helper() -> u32 {
    let a = 1;
    let b = 2;
    twice!(a + b) + unsafe { Bits { int: 0 }.int }
}

#[allow(dead_code)]
pub mod shapes {
    pub fn unit_side() -> u32 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_runs() {
        let engine = Engine { counter: Counter::new() };
        assert!(run(&engine) > 0);
    }
}
