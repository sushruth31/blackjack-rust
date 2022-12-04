mod utils;
use rand::Rng;
use std::{borrow::Borrow, collections::HashMap, sync::Arc};
use utils::*;

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::{mpsc, Mutex},
};

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

type PlayerChannels = Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>;

#[tokio::main]
async fn main() {
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
        player_channels.iter().for_each(|(id, tx)| {
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
                let channel = channels.get_mut(&player.id).unwrap();
                //send the player a message that they have joined the game
                channel
                    .send(format!("You have joined the game, {}!\n", player.name))
                    .await
                    .unwrap();
                //need to drop the lock on the channels before broadcasting
                drop(channels);
                drop(game);
                //send all other players a message that a new player has joined
                broadcast(
                    format!("Testing the brocast. {} joined the game\n", player.name,),
                    player.id.clone(),
                )
                .await;
                //loop through all the players and deal them cards
                let channels = player_channels.lock().await;
                let tx = channels.get(&player.id).unwrap();
                tx.send(format!(
                    "Ok, {}, it's your turn. Place a bet\n",
                    player.name
                ))
                .await
                .unwrap();
                //await the player to send a message back that is a number
            }
        }
    });

    let listener = TcpListener::bind("localhost:8080").await.unwrap();
    loop {
        let (mut stream, id) = listener.accept().await.unwrap();
        let lobby = Arc::clone(&player_lobby_clone);
        let player_channels_clone = Arc::clone(&player_channels_clone);
        tokio::spawn(async move {
            let mut name = [0; 32];
            stream
                .write_all("Welcome to blackjack. Please type your name to proceed \n".as_bytes())
                .await
                .unwrap();
            //read the name of the player
            stream.read(&mut name).await.unwrap();
            let name = String::from_utf8(name.to_vec()).unwrap();
            //add the player to the lobby
            let mut lobby = lobby.lock().await;
            let mut player_channels = player_channels_clone.lock().await;
            //create a channel for the player and add it to the hashmap
            let (tx, mut rx) = mpsc::channel(32);
            player_channels.insert(id.to_string(), tx);
            lobby.push(Player::new(&name, &id.to_string()));
            stream
                .write_all(
                    "You have joined the lobby. Waiting for other players to join... \n".as_bytes(),
                )
                .await
                .unwrap();
            drop(lobby);
            drop(player_channels);
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                tokio::select! {
                    Some(msg) = rx.recv() => {
                        //write to the stream
                        writer.write_all(msg.as_bytes()).await.unwrap();
                    }
                    Ok(result) = reader.read_line(&mut line) => {
                        if result == 0 {
                            println!("{} disconnected", id);
                            break;
                        }
                        writer.write_all(line.as_bytes()).await.unwrap();
                    }
                }
            }
            //add the player to the lobby
        });
    }
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
