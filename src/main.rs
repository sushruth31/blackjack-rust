use rand::Rng;
use std::cell::RefCell;

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::broadcast,
};

#[derive(Debug, Clone)]
struct Player {
    name: String,
    money: i32,
    current_bet: i32,
    id: String,
}

impl Player {
    fn new(name: &str, id: &str) -> Self {
        Self {
            name: name.to_string(),
            money: 100,
            current_bet: 0,
            id: id.to_string(),
        }
    }
}

#[derive(Clone)]
struct Game {
    player_pool: Vec<Player>,
    deck: Deck,
    is_in_progress: bool,
}

impl Game {
    fn add_player(&mut self, name: String, id: String) {
        self.player_pool.push(Player {
            name,
            money: 100,
            current_bet: 0,
            id,
        });
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("localhost:8080").await.unwrap();
    let (tx, rx) = broadcast::channel::<String>(10);
    let player_lobby: RefCell<Vec<Player>> = RefCell::new(Vec::new());
    let mut deck = Deck::new();
    deck.shuffle();
    let mut game = Game {
        player_pool: Vec::new(),
        deck,
        is_in_progress: false,
    };
    loop {
        let (mut socket, id) = listener.accept().await.unwrap();
        //add new line to the end of the message
        //write to socket welcome message
        socket
            .write_all(b"\r\nWelcome to the game! Please enter your name to continue\r\n")
            .await
            .unwrap();
        let mut reader = BufReader::new(&mut socket);
        let mut usr_name = String::new();
        reader.read_line(&mut usr_name).await.unwrap();
        let player = Player::new(&usr_name, &id.to_string());
        socket
            .write_all(format!("Welcome {}", usr_name).as_bytes())
            .await
            .unwrap();
        //if game is started, send message to player that game is already started and add to lobby
        //if game is not started, add player to game
        if game.is_in_progress {
            //send message to player that game is already started
            tx.send(format!("Game is already started")).unwrap();
            player_lobby.borrow_mut().push(player);
        } else {
            game.add_player(usr_name, id.to_string());
        }

        //start the game here
        loop {
            game.is_in_progress = true;
            let mut player_lobby = player_lobby.borrow_mut();
            //move players from lobby to game
            if !player_lobby.is_empty() {
                for player in player_lobby.iter() {
                    game.add_player(player.name.clone(), player.id.clone());
                    socket
                        .write_all(format!("{} joined the game", player.name).as_bytes())
                        .await
                        .unwrap();
                }
                player_lobby.clear();
            }
            //create players to loop through including dealer
            let mut players = game.player_pool.clone();
            players.push(Player::new("Dealer", "dealer"));
            //check if there is at least one player with money if not, end the game
            if players.iter().all(|player| player.money == 0) {
                game.is_in_progress = false;
                break;
            }
        }
    }
}

#[derive(Clone)]
struct Card {
    suit: String,
    value: i32,
}

#[derive(Clone)]
struct Deck(Vec<Card>);

impl Deck {
    fn new() -> Self {
        let nums = vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        let symbols = vec!["♠", "♥", "♦", "♣"];
        let mut deck: Vec<Card> = Vec::new();
        for num in nums {
            for symbol in &symbols {
                deck.push(Card {
                    suit: symbol.to_string(),
                    value: num,
                });
            }
        }
        return Self(deck);
    }

    fn shuffle(&mut self) {
        let cards = &mut self.0;
        for i in 0..cards.len() {
            let rand = rand::thread_rng().gen_range(0..cards.len());
            cards.swap(i, rand);
        }
    }
}
