use std::ops::BitXor;

#[derive(Debug, PartialEq)]
enum Operation {
    ReverseBits,
    Xor(u8),
    XorPos,
    Add(u8),
    AddPos,
}

impl Operation {
    // Execute the operation
    fn execute(&self, byte: u8, position: u8) -> u8 {
        match self {
            Self::ReverseBits => byte.reverse_bits(),
            Self::Xor(number) => byte.bitxor(number),
            Self::XorPos => byte.bitxor(position),
            Self::Add(number) => byte.wrapping_add(*number),
            Self::AddPos => byte.wrapping_add(position),
        }
    }

    // Reverse the execution of the operation
    // in other words: reverse_execute(execute(byte)) == byte
    fn reverse_execute(&self, byte: u8, position: u8) -> u8 {
        match self {
            Self::AddPos => byte.wrapping_sub(position),
            Self::Add(number) => byte.wrapping_sub(*number),
            // the rest of the operations are symmetric
            _ => self.execute(byte, position),
        }
    }
}

#[derive(Debug)]
pub struct Spec {
    ops: Vec<Operation>,
}

impl Spec {
    fn encrypt_byte(&self, byte: u8, position: u8) -> u8 {
        let mut result = byte;
        for op in self.ops.iter() {
            result = op.execute(result, position);
        }
        result
    }

    pub fn encrypt(&self, data: &mut [u8], counter: usize) {
        let counter = usize_to_mod_u8_field(counter);

        for (idx, byte) in data.iter_mut().enumerate() {
            let idx = usize_to_mod_u8_field(idx);
            let position = counter.wrapping_add(idx);
            *byte = self.encrypt_byte(*byte, position)
        }
    }

    fn decrypt_byte(&self, byte: u8, position: u8) -> u8 {
        let mut result = byte;
        for op in self.ops.iter().rev() {
            result = op.reverse_execute(result, position);
        }
        result
    }

    pub fn decrypt(&self, data: &mut [u8], counter: usize) {
        let counter = usize_to_mod_u8_field(counter);

        for (idx, byte) in data.iter_mut().enumerate() {
            let idx = usize_to_mod_u8_field(idx);
            let position = counter.wrapping_add(idx);
            *byte = self.decrypt_byte(*byte, position)
        }
    }

    // check if the spec is algorithmically equal to no-op
    pub fn is_noop(&self) -> bool {
        // we know that spec is algorithmically equal to no-op iff for
        // every byte and position it'll return the byte itself.
        for byte in 0..u8::MAX {
            for position in 0..u8::MAX {
                if byte != self.encrypt_byte(byte, position) {
                    // we found a pair that proves it's not a no-op
                    return false;
                }
            }
        }

        true
    }
}

// converts a usize into the mod_u8 field
fn usize_to_mod_u8_field(value: usize) -> u8 {
    (value % (u8::MAX as usize + 1)) as u8
}

#[derive(thiserror::Error, Debug)]
pub enum CipherParseErr {
    #[error("Does not recognize operation: {0:X?}")]
    UnknownOperation(u8),

    #[error("Received an EOF while reading operation: {0:X?}")]
    UnexpectedEOF(u8),
}

impl TryFrom<&[u8]> for Spec {
    type Error = CipherParseErr;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let mut ops = Vec::new();

        let mut bytes = value.iter();
        while let Some(op) = bytes.next() {
            match op {
                0x01 => ops.push(Operation::ReverseBits),
                0x02 => {
                    let number = *bytes.next().ok_or(CipherParseErr::UnexpectedEOF(*op))?;
                    ops.push(Operation::Xor(number));
                }
                0x03 => ops.push(Operation::XorPos),
                0x04 => {
                    let number = *bytes.next().ok_or(CipherParseErr::UnexpectedEOF(*op))?;
                    ops.push(Operation::Add(number));
                }
                0x05 => ops.push(Operation::AddPos),
                _ => return Err(CipherParseErr::UnknownOperation(*op)),
            }
        }

        Ok(Self { ops })
    }
}

#[cfg(test)]
mod tests {
    use super::{Operation, Spec};

    #[test]
    fn parse_spec_correctly() {
        let raw_spec: &[u8] = b"\x01\x02\x7b\x03\x04\x3e\x05";
        let parsed_spec: Spec = raw_spec.try_into().unwrap();
        let expected_spec = Spec {
            ops: [
                Operation::ReverseBits,
                Operation::Xor(0x7b),
                Operation::XorPos,
                Operation::Add(0x3e),
                Operation::AddPos,
            ]
            .into(),
        };

        assert_eq!(parsed_spec.ops, expected_spec.ops);
    }

    #[test]
    fn encrypt_correctly() {
        fn check_encrypt(input: &[u8], spec: &[u8], expected_output: &[u8]) {
            let spec: Spec = spec.try_into().unwrap();
            let mut output = input.to_vec();
            spec.encrypt(&mut output, 0);
            assert_eq!(output, expected_output)
        }

        check_encrypt("hello".as_bytes(), b"\x02\x01\x01", b"\x96\x26\xb6\xb6\x76");
        check_encrypt("hello".as_bytes(), b"\x05\x05", b"\x68\x67\x70\x72\x77");
        check_encrypt(
            "4x dog,5x car\n3x rat,2x cat\n".as_bytes(),
            b"\x02\x7b\x05\x01",
            b"\xf2\x20\xba\x44\x18\x84\xba\xaa\xd0\x26\x44\xa4\xa8\x7e\x6a\x48\xd6\x58\x34\x44\xd6\x7a\x98\x4e\x0c\xcc\x94\x31",
        );
        check_encrypt(
            "5x car\n3x rat\n".as_bytes(),
            b"\x02\x7b\x05\x01",
            b"\x72\x20\xba\xd8\x78\x70\xee\xf2\xd0\x26\xc8\xa4\xd8\x7e",
        );
    }

    #[test]
    fn decrypt_correctly() {
        fn check_decrypt(input: &[u8], spec: &[u8], expected_output: &[u8]) {
            let spec: Spec = spec.try_into().unwrap();
            let mut output = input.to_vec();
            spec.decrypt(&mut output, 0);
            assert_eq!(output, expected_output)
        }

        check_decrypt(b"\x96\x26\xb6\xb6\x76", b"\x02\x01\x01", "hello".as_bytes());
        check_decrypt(b"\x68\x67\x70\x72\x77", b"\x05\x05", "hello".as_bytes());
        check_decrypt(
            b"\xf2\x20\xba\x44\x18\x84\xba\xaa\xd0\x26\x44\xa4\xa8\x7e\x6a\x48\xd6\x58\x34\x44\xd6\x7a\x98\x4e\x0c\xcc\x94\x31",
            b"\x02\x7b\x05\x01",
            "4x dog,5x car\n3x rat,2x cat\n".as_bytes(),
        );
        check_decrypt(
            b"\x72\x20\xba\xd8\x78\x70\xee\xf2\xd0\x26\xc8\xa4\xd8\x7e",
            b"\x02\x7b\x05\x01",
            "5x car\n3x rat\n".as_bytes(),
        );
    }

    #[test]
    fn noop_detection() {
        let noop_specs: &[&[u8]] = &[
            b"",
            b"\x02\x00",
            b"\x02\xab\x02\xab",
            b"\x01\x01",
            b"\x02\xa0\x02\x0b\x02\xab",
            b"\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab\x02\xa0\x02\x0b\x02\xab",
        ];

        for &spec in noop_specs {
            let spec: Spec = spec.try_into().unwrap();
            assert!(spec.is_noop())
        }

        let not_noop_specs: &[&[u8]] = &[
            b"\x02\x01\x01",
            b"\x05\x05",
            b"\x02\x7b\x05\x01",
            b"\x02\xa0\x02\x0b\x02\xab\x02\x7b\x02\xa0\x02\x0b\x02\xab\x05\x02\xa0\x02\x0b\x02\xab\x01\x02\xa0\x02\x0b\x02\xab",
        ];

        for &spec in not_noop_specs {
            let spec: Spec = spec.try_into().unwrap();
            assert!(!spec.is_noop())
        }
    }
}
