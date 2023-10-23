use protocol::MALFORMED_RESPONSE;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

mod protocol;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(serve(conn));
    }
}

async fn serve(mut client: TcpStream) {
    let (reader, mut writer) = client.split();
    let mut reader = BufReader::new(reader);
    loop {
        let mut line = String::new();
        let rcount = reader
            .read_line(&mut line)
            .await
            .expect("reading from socket");
        if rcount == 0 {
            // reached EOF
            return;
        }

        match protocol::get_number_from_request(&line) {
            Err(_) => {
                // received a bad request, return a malformed response and close the socket
                writer
                    .write_all(MALFORMED_RESPONSE.as_bytes())
                    .await
                    .expect("write to socket");
                return;
            }
            Ok(number) => {
                let response = protocol::Response::new(number.map(is_prime).unwrap_or(false));
                let response =
                    serde_json::to_string(&response).expect("failed to serialize response") + "\n";

                writer
                    .write_all(response.as_bytes())
                    .await
                    .expect("write to socket");
            }
        }
    }
}

fn is_prime(number: u64) -> bool {
    if number == 2 || number == 3 {
        return true;
    }

    if number <= 1 {
        return false;
    }

    for div in 2..number {
        if div * div > number {
            break;
        }

        if number % div == 0 {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::is_prime;

    #[test]
    fn check_is_prime() {
        // primes
        assert!(is_prime(2));
        assert!(is_prime(3));
        assert!(is_prime(5));
        assert!(is_prime(13));
        assert!(is_prime(8191));

        // not primes
        assert!(!is_prime(0));
        assert!(!is_prime(1));
        assert!(!is_prime(4));
        assert!(!is_prime(6));
        assert!(!is_prime(45));
    }
}
