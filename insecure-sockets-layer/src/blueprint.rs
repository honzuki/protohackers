use std::{num::ParseIntError, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Toy {
    count: usize,
    text: String,
}

#[derive(thiserror::Error, Debug)]
pub enum ToyParseErr {
    #[error("Unknown toy's blueprint format")]
    UnknwonFormat,

    #[error("Failed to parse the count: {0}")]
    UnknownNumberFormat(#[from] ParseIntError),
}

impl ToString for Toy {
    fn to_string(&self) -> String {
        self.count.to_string() + "x " + &self.text
    }
}

impl FromStr for Toy {
    type Err = ToyParseErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('x').collect();
        if parts.len() != 2 {
            return Err(ToyParseErr::UnknwonFormat);
        }

        let count = parts[0].parse()?;
        Ok(Self {
            count,
            text: parts[1].trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Toy;

    #[test]
    fn check_parsing() {
        let raw_toys = ["4x dog", "5x big wagon"];
        let expected_toys = [
            Toy {
                count: 4,
                text: "dog".into(),
            },
            Toy {
                count: 5,
                text: "big wagon".into(),
            },
        ];

        for (raw_toy, expected_toy) in raw_toys.iter().zip(expected_toys.iter()) {
            let parsed_toy: Toy = raw_toy.parse().unwrap();
            assert_eq!(parsed_toy, *expected_toy);
        }
    }
}
