//! Engine fixture: the integration tests split this file and compile the result.

mod child;

use child::Counter;

pub struct Engine {
    counter: Counter,
}

pub fn run(engine: &Engine) -> u32 {
    engine.alpha_start() + gamma_helper()
}

#[allow(unused_mut)]
// comment: between impl Engine attributes
#[cfg(not(any()))]
impl Engine { // comment: after impl Engine brace
    #![allow(dead_code)]

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

pub struct Wrapper<T> {
    inner: T,
}

impl<T> /* x */ Wrapper<T> where T: Clone // y
{
    pub fn wrap_first(&self) -> T {
        let first = self.inner.clone();
        let second = first.clone();
        let third = second.clone();
        third
    }

    pub fn wrap_second(&self) -> (T, T) {
        let left = self.wrap_first();
        let right = self.inner.clone();
        let pair = (left, right);
        pair
    }
}

impl<T> Wrapper<T>
where
    // comment: in where clause
    T: Clone
        + Default,
{
    pub fn zeta_reset(&mut self) -> T {
        let old = self.inner.clone();
        let fresh = T::default();
        self.inner = fresh;
        old
    }

    pub fn zeta_swap(&mut self, value: T) -> T {
        let old = self.zeta_reset();
        let kept = old.clone();
        self.inner = value;
        kept
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
mod tests { // comment: after tests brace
    #![allow(unused_imports)]

    // comment: before use super
    use super::*;

    // comment: before delta_runs
    #[test]
    fn delta_runs() {
        let engine = Engine { counter: Counter::new() };
        assert!(run(&engine) > 0);
    }

    // comment: before epsilon_runs
    #[test]
    fn epsilon_runs() {
        assert_eq!(gamma_helper(), 6);
    }

    // comment: end of tests
}
