mod utils;
use rand::Rng;
use std::{cell::RefCell, sync::Arc};
use utils::*;

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::{broadcast, Mutex},
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
    fn add_player(&mut self, name: String, id: String) {
        self.player_pool.push(Player {
            name,
            money: 100,
            current_bet: 0,
            id,
            cards: Vec::new(),
        });
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("localhost:8080").await.unwrap();
    let (tx, mut rx) = broadcast::channel::<String>(10);
    let tx_clone = tx.clone();
    let player_lobby: Arc<Mutex<Vec<Player>>> = Arc::new(Mutex::new(Vec::new()));
    let player_lobby_clone = player_lobby.clone();
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
                game.add_player(player.name, player.id);
                //notify the player that they have been added to the game
                tx.send(format!("You have been added to the game!"))
                    .unwrap();
            }
            //loop through all the players and deal them cards
            loop {
                //deal card to dealer
                game.dealer.cards.push(game.deck.deal_card());
                for player in &mut game.player_pool {
                    //display  dealers cards and the players cards
                    tx.send(format!("Dealer's hand")).unwrap();
                    for card in &game.dealer.cards {
                        tx.send(format!("{}", format_card(card))).unwrap();
                    }
                    loop {
                        tx.send(format!("Place a bet!")).unwrap();
                        //wait for a response
                        let resp = rx.recv().await.unwrap();
                        if let Ok(resp) = resp.parse::<i32>() {
                            if player.money >= resp {
                                player.current_bet = resp;
                                player.money -= resp;
                                tx.send(format!("Bet placed! You have {} left", player.money))
                                    .unwrap();
                                //deal the player a card
                                player.cards.push(game.deck.deal_card());
                                break;
                            }
                        } else {
                            tx.send(format!("Please enter a number!")).unwrap();
                        }
                    }
                }
            }
        }
    });

    loop {
        let (mut socket, id) = listener.accept().await.unwrap();

        //add new line to the end of the message
        tx_clone
            .send(format!("Welcome to the game! Type your name"))
            .unwrap();
        let mut reader = BufReader::new(&mut socket);
        let mut usr_name = String::new();
        reader.read_line(&mut usr_name).await.unwrap();
        let player = Player::new(&usr_name, &id.to_string());
        socket
            .write_all(format!("{} has joined the lobby!", player.name).as_bytes())
            .await
            .unwrap();
        //add player to lobby
        player_lobby_clone.lock().await.push(player);
        //spawn a new task for each player
        tokio::spawn(async move {
            let mut reader = BufReader::new(&mut socket);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
            }
        });
    }
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
