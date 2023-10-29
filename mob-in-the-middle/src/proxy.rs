use tokio::io::AsyncWriteExt;

const TONYS_ADDR: &str = "7YWHMfk9JZe0LM0g1ZauHuiSxhI";

pub struct Writer<W> {
    writer: W,
}

impl<W> Writer<W>
where
    W: AsyncWriteExt + Unpin,
{
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub async fn write(&mut self, message: &str) -> tokio::io::Result<()> {
        println!("received: {:?}\n\"{}\"", message.as_bytes(), message);

        // combain all the parts back into a single message again
        let modified_message = message
            .split(' ')
            .map(|part| map_address(part.to_string()))
            .collect::<Vec<_>>()
            .join(" ");

        println!(
            "sent: {:?}\n\"{}\"",
            modified_message.as_bytes(),
            modified_message
        );

        self.writer.write_all(modified_message.as_bytes()).await?;
        self.writer.flush().await?;

        Ok(())
    }
}

fn map_address(message: String) -> String {
    if !is_boguscoin_addr(message.trim()) {
        return message;
    }

    message.replace(message.trim(), TONYS_ADDR)
}

fn is_boguscoin_addr(text: &str) -> bool {
    text.len() >= 26
        && text.len() <= 35
        && text.starts_with('7')
        && text.chars().all(|ch| ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use crate::proxy::is_boguscoin_addr;

    #[test]
    fn check_is_bogus_address() {
        let valid_addresses = [
            "7F1u3wSD5RbOHQmupo9nx4TnhQ",
            "7iKDZEwPZSqIvDnHvVN2r0hUWXD5rHX",
            "7LOrwbDlS8NujgjddyogWgIM93MV5N2VR",
            "7adNeSwJkMakpEcln9HEtthSRtxdmEHOT8T",
        ];

        for addr in valid_addresses {
            assert!(is_boguscoin_addr(addr))
        }

        let invalid_addresses = [
            "7F1u3wSD5RbOHQmupo9",
            "8iKDZEwPZSqIvDnHvVN2r0hUWXD5rHX",
            "7adNeSwJkMakpEcln9HEtthSRtxdmEHOT8T7adNeSwJkMakpEcln9HEtthSRtxdmEHOT8T",
            "7LOrwbDlS8Nujgj gWgIM93MV5N2VR",
        ];

        for addr in invalid_addresses {
            assert!(!is_boguscoin_addr(addr));
        }
    }
}
