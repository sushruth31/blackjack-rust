use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::broadcast,
};

struct Player {
    name: String,
    money: i32,
    current_bet: i32,
}

struct Game {
    player_pool: Vec<Player>,
    deck: Vec<i32>,
}

impl Game {
    fn add_player(&mut self, name: String) {
        self.player_pool.push(Player {
            name,
            money: 100,
            current_bet: 0,
        });
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("localhost:8080").await.unwrap();
    let (tx, rx) = broadcast::channel::<String>(10);
    let mut game = Game {
        player_pool: Vec::new(),
        deck: Vec::new(),
    };
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        //add new line to the end of the message
        //write to socket welcome message
        socket
            .write_all(b"\r\nWelcome to the game! Please enter your name to continue\r\n")
            .await
            .unwrap();
        let mut reader = BufReader::new(&mut socket);
        let mut usr_name = String::new();
        reader.read_line(&mut usr_name).await.unwrap();
        socket
            .write_all(format!("Welcome {}", usr_name).as_bytes())
            .await
            .unwrap();
        //add player to the player pool
        game.add_player(usr_name);
        //print all the players in the pool
        for player in &game.player_pool {
            println!("Player: {}", player.name);
        }

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                let n = reader.read_line(&mut line).await.unwrap();
                if n == 0 {
                    break;
                }
                writer.write_all(line.as_bytes()).await.unwrap();
                line.clear();
            }
        });
    }
}
