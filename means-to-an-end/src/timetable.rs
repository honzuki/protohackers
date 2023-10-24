use std::collections::BTreeMap;

#[derive(Default)]
pub struct Table(BTreeMap<i32, i32>);

impl Table {
    // Sets the price at the given timestamp
    // if it wasn't set before, otherwise does nothing.
    pub fn set_price(&mut self, timestamp: i32, price: i32) {
        self.0.entry(timestamp).or_insert(price);
    }

    // Returns the average price over a time period, rounded down
    pub fn average(&self, min_time: i32, max_time: i32) -> i32 {
        if min_time > max_time {
            return 0;
        }
        let mut avg = 0f64;
        for (idx, (_, price)) in self.0.range(min_time..=max_time).enumerate() {
            avg += (*price as f64 - avg) / (idx + 1) as f64;
        }

        avg as i32
    }
}

#[cfg(test)]
mod tests {
    use super::Table;

    #[test]
    fn check_normal_flow() {
        let mut table = Table::default();
        table.set_price(12345, 101);
        table.set_price(12346, 102);
        table.set_price(12347, 100);
        table.set_price(40960, 5);
        assert_eq!(table.average(12288, 16384), 101);
    }

    #[test]
    fn check_minus_numbers() {
        let mut table = Table::default();
        table.set_price(-650, -69);
        table.set_price(-250, 102);
        table.set_price(-1000, 100);
        table.set_price(400, -80);
        table.set_price(20, 80);
        table.set_price(500, 8);
        table.set_price(-1020, -90);
        table.set_price(-360, 100);
        assert_eq!(table.average(-400, 1000), 42);
    }

    #[test]
    fn bad_range() {
        let mut table = Table::default();
        table.set_price(-650, -69);
        table.set_price(-250, 102);
        table.set_price(-1000, 100);
        assert_eq!(table.average(899999, 1000), 0);
    }
}
