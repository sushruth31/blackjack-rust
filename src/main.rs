mod utils;
use rand::Rng;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
};
use utils::*;

use tokio::{
    io::AsyncWriteExt,
    net::TcpStream,
    sync::{mpsc, Mutex},
};

#[derive(Clone)]
struct Player {
    name: String,
    money: i32,
    current_bet: i32,
    id: String,
    cards: Vec<Card>,
}

impl Player {
    fn new(name: &str, id: &str) -> Self {
        Self {
            name: name.to_string(),
            money: 100,
            current_bet: 0,
            id: id.to_string(),
            cards: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct Dealer {
    cards: Vec<Card>,
}

#[derive(Clone)]
struct Game {
    player_pool: Vec<Player>,
    deck: Deck,
    dealer: Dealer,
}

impl Game {
    fn add_player(&mut self, name: &str, id: &str) {
        self.player_pool.push(Player {
            name: name.to_string(),
            money: 100,
            current_bet: 0,
            id: id.to_string(),
            cards: Vec::new(),
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("localhost:8080")?;
    let player_lobby: Arc<Mutex<Vec<Player>>> = Arc::new(Mutex::new(Vec::new()));
    let player_channels: Arc<Mutex<HashMap<String, tokio::sync::mpsc::Sender<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let player_lobby_clone = player_lobby.clone();
    let player_channels_clone = player_channels.clone();
    let mut deck = Deck::new();
    deck.shuffle();
    let mut game = Game {
        player_pool: Vec::new(),
        deck,
        dealer: Dealer { cards: Vec::new() },
    };

    tokio::spawn(async move {
        //move players from lobby to game
        loop {
            //if no players in lobby wait
            if player_lobby.lock().await.len() == 0 {
                continue;
            }
            if game.deck.0.len() < 10 {
                game.deck = Deck::new();
                game.deck.shuffle();
            }
            let mut players = player_lobby.lock().await;
            for player in players.drain(..) {
                game.add_player(&player.name, &player.id);
                println!("{} {}, joined the game", player.name, player.id);
                //get the channel for the player to send messages to
                let mut channels = player_channels.lock().await;
                let channel = channels.get_mut(&player.id).unwrap();
                //send the player a message that they have joined the game
                channel
                    .send("You have joined the game".to_string())
                    .await
                    .unwrap();
            }
            //loop through all the players and deal them cards
        }
    });

    for stream in listener.incoming() {
        let lobby = Arc::clone(&player_lobby_clone);
        let player_channels_clone = Arc::clone(&player_channels_clone);
        tokio::spawn(async move {
            let mut stream = stream.unwrap();
            let mut name = [0; 32];
            stream
                .write("Welcome to blackjack. Please type your name to proceed \n".as_bytes())
                .unwrap();
            //read the name of the player
            stream.read(&mut name).unwrap();
            let name = String::from_utf8(name.to_vec()).unwrap();
            //add the player to the lobby
            let id = stream.peer_addr().unwrap().to_string();
            let mut lobby = lobby.lock().await;
            let mut player_channels = player_channels_clone.lock().await;
            //create a channel for the player and add it to the hashmap
            let (tx, mut rx) = mpsc::channel(32);
            player_channels.insert(id.clone(), tx);
            lobby.push(Player::new(&name, &id));
            //print players in lobby
            for player in lobby.iter() {
                println!("{} is in the lobby", player.name);
            }
            std::mem::drop(lobby);
            std::mem::drop(player_channels);
            loop {
                //read messages from channel and send them to the player
                if let Some(message) = rx.recv().await {
                    stream.write(message.as_bytes()).unwrap();
                }
            }
            //add the player to the lobby
        });
    }
    Ok(())
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

    fn deal_card(&mut self) -> Card {
        self.0.pop().unwrap()
    }
}
