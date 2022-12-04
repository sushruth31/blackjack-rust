mod utils;
use rand::Rng;
use std::{
    borrow::{Borrow, BorrowMut},
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    pin::Pin,
    sync::Arc,
};
use utils::*;

use tokio::sync::{mpsc, Mutex};

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
struct Dealer {
    cards: Vec<Card>,
}

#[derive(Clone, Debug)]
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

type PlayerChannels = Arc<
    Mutex<
        HashMap<
            String,
            (
                mpsc::Sender<String>,
                Arc<Mutex<mpsc::Receiver<String>>>,
                TcpStream,
            ),
        >,
    >,
>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("localhost:8080")?;
    let player_lobby: Arc<Mutex<Vec<Player>>> = Arc::new(Mutex::new(Vec::new()));
    let player_lobby_clone = player_lobby.clone();
    let player_channels: PlayerChannels = Arc::new(Mutex::new(HashMap::new()));
    let player_channels_clone = player_channels.clone();
    let player_channels_clone2 = player_channels.clone();
    let mut deck = Deck::new();
    deck.shuffle();
    let game = Arc::new(Mutex::new(Game {
        player_pool: Vec::new(),
        deck,
        dealer: Dealer { cards: Vec::new() },
    }));

    //function that will send to all players in the game
    let broadcast = |msg: String, from: String| async move {
        let player_channels = Arc::clone(&player_channels_clone2);
        let player_channels = player_channels.lock().await;
        player_channels.iter().for_each(|(id, (tx, rx, stream))| {
            if id != &from {
                let res = tx.try_send(msg.clone());
                if res.is_err() {
                    println!("Error sending to player {}", id);
                } else {
                    println!("Sent to player {}", id);
                }
            }
        });
    };

    tokio::spawn(async move {
        let game = Arc::clone(&game);
        //move players from lobby to game
        loop {
            let mut game = game.lock().await;
            //if no players in lobby wait
            if player_lobby.lock().await.len() == 0 {
                continue;
            }
            if game.borrow().deck.0.len() < 10 {
                game.deck = Deck::new();
                game.deck.shuffle();
            }
            //give the dealer a card
            if let Some(card) = game.deck.deal_card() {
                game.dealer.cards.push(card);
            } else {
                continue;
            }
            let mut players = player_lobby.lock().await;
            for player in players.drain(..) {
                let mut game = game.clone();
                let broadcast = broadcast.clone();
                game.add_player(&player.name, &player.id);
                println!("{} {}, joined the game", player.name, player.id);
                //get the channel for the player to send messages to
                let mut channels = player_channels.lock().await;
                let (channel, rx, stream) = channels.get_mut(&player.id).unwrap();
                //send the player a message that they have joined the game
                channel
                    .send(format!("You have joined the game, {}!", player.name))
                    .await
                    .unwrap();
                //need to drop the lock on the channels before broadcasting
                drop(channels);
                drop(game);
                //send all other players a message that a new player has joined
                broadcast(
                    format!("Testing the brocast. {} joined the game", player.name,),
                    player.id.clone(),
                )
                .await;
                //loop through all the players and deal them cards
                let channels = player_channels.lock().await;
                let (tx, rx, stream) = channels.get(&player.id).unwrap();
                let mut rx = rx.lock().await;
                tx.send(format!("Ok, {}, it's your turn. Place a bet", player.name))
                    .await
                    .unwrap();
                //await the player to send a message back that is a number
                let mut buf = [0; 1024];
                let mut stream = stream.try_clone().unwrap();
                let mut bet = 0;
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    let msg = msg.trim();
                    if let Ok(_bet) = msg.parse::<i32>() {
                        if _bet > 0 {
                            tx.send(format!("You bet {}", _bet)).await.unwrap();
                            bet = _bet;
                            break;
                        }
                    }
                    tx.send("Please enter a valid bet".to_owned())
                        .await
                        .unwrap();
                }
            }
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
            let (tx, rx) = mpsc::channel(32);
            let rx = Arc::new(Mutex::new(rx));
            let rx_copy = rx.clone();
            let stream_copy = stream.try_clone().unwrap();
            player_channels.insert(id.clone(), (tx, rx_copy, stream_copy));
            lobby.push(Player::new(&name, &id));
            drop(lobby);
            drop(player_channels);
            loop {
                let mut rx = rx.lock().await;
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

#[derive(Clone, Debug)]
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

    fn deal_card(&mut self) -> Option<Card> {
        return self.0.pop();
    }
}
