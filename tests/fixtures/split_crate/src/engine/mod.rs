//! Engine fixture: the integration tests split this file and compile the result.

mod child;

use child::Counter;

pub struct Engine {
    counter: Counter,
}

pub fn run(engine: &Engine) -> u32 {
    engine.alpha_start() + gamma_helper()
}

impl Engine {
    fn alpha_start(&self) -> u32 {
        let first = self.counter.value();
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

fn gamma_helper() -> u32 {
    let a = 1;
    let b = 2;
    a + b
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
