use chatroom::ChatRoom;
use protocol::JoinSuccess;
use tokio::net::{TcpListener, TcpStream};

use crate::protocol::FromChatRoomMessage;

mod chatroom;
mod client;
mod protocol;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    println!("Server listening on: {}", listener.local_addr().unwrap());

    let chatroom = ChatRoom::create();

    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(handle_connection(conn, chatroom.clone()));
    }
}

async fn handle_connection(mut client: TcpStream, chatroom: ChatRoom) -> anyhow::Result<()> {
    let (reader, writer) = client.split();
    let mut reader = client::Reader::new(reader);
    let mut writer = client::Writer::new(writer);

    // Register a new user
    writer.send_welcome_message().await?;
    let username = reader.read_name().await?;
    let (
        chatroom,
        JoinSuccess {
            userlist,
            rx: mut from_chat_room,
        },
    ) = chatroom.register(username.trim().to_owned()).await?;

    // Send the user list
    writer.send_user_list(userlist).await?;

    // Handle new messages from the user
    let from_user = async move {
        loop {
            let message = match reader.read_message().await {
                Ok(message) => message,
                Err(client::ReaderError::Eof) => break,
                Err(err) => Err(err)?,
            };

            chatroom.send_message(message.trim().to_owned()).await?;
        }

        // the user has disconnected, leave the room
        chatroom.leave().await?;

        Ok::<(), anyhow::Error>(())
    };

    // Handle new messages from the server
    let to_user = async move {
        while let Some(message) = from_chat_room.receiver.recv().await {
            match message {
                FromChatRoomMessage::Join(username) => writer.send_join_message(&username).await?,
                FromChatRoomMessage::Leave(username) => writer.send_left_message(&username).await?,
                FromChatRoomMessage::ChatMessage(from, message) => {
                    writer.send_message(&from, &message).await?
                }
            }
        }

        // the chat room has terminated the client
        // we don't need to notify the user and can let the socket terminate

        Ok::<(), anyhow::Error>(())
    };

    // Terminate once any of the streams reaches EOF
    tokio::select! {
        _ = from_user => {}
        _ = to_user => {}
    };

    Ok(())
}
