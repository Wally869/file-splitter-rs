pub struct Counter {
    value: u32,
}

impl Counter {
    pub fn new() -> Self {
        Counter { value: 1 }
    }

    pub fn value(&self) -> u32 {
        self.value
    }
}
