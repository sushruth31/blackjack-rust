mod utils;
use std::{borrow::Borrow, collections::HashMap, sync::Arc};
use utils::*;

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::{mpsc, Mutex},
};

type PlayerChannels = Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>;

#[tokio::main]
async fn main() {
    let player_lobby: Arc<Mutex<Vec<Player>>> = Arc::new(Mutex::new(Vec::new()));
    let player_bet_pool: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let player_bet_pool_clone = player_bet_pool.clone();
    let player_lobby_clone = player_lobby.clone();
    let player_channels: PlayerChannels = Arc::new(Mutex::new(HashMap::new()));
    let current_player: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let player_channels_clone = player_channels.clone();
    let player_channels_clone2 = player_channels.clone();
    let mut deck = Deck::new();
    deck.shuffle();
    let game = Arc::new(Mutex::new(Game {
        player_pool: Vec::new(),
        deck,
        dealer: Dealer { cards: Vec::new() },
        in_progress: false,
    }));
    let current_player_clone = current_player.clone();

    //function that will send to all players in the game
    let broadcast = |msg: String, from: String| async move {
        let player_channels = Arc::clone(&player_channels_clone2);
        let player_channels = player_channels.lock().await;
        player_channels.iter().for_each(|(id, tx)| {
            if id != &from {
                let res = tx.try_send(msg.clone());
                if res.is_err() {
                    println!("Error sending to player {}", id);
                }
            }
        });
    };

    let send_tx: Box<dyn Fn(&str, &mpsc::Sender<String>) + Send> = Box::new(|msg, tx| {
        let res = tx.try_send(msg.to_string());
        if res.is_err() {
            println!("Error sending to player");
        }
    });

    tokio::spawn(async move {
        let game = Arc::clone(&game);
        //move players from lobby to game
        loop {
            let mut game = game.lock().await;
            //wait for at least 2 players in the lobby to start the game
            let mut lobbyplayers = player_lobby.lock().await;
            if lobbyplayers.len() >= 2 {
                continue;
            }
            if game.borrow().deck.0.len() < 10 {
                game.deck = Deck::new();
                game.deck.shuffle();
            }
            //add each player to the game.
            for player in lobbyplayers.drain(..) {
                let mut channels = player_channels.lock().await;
                let channel = channels.get_mut(&player.id).unwrap();
                channel
                    .send(format!("You have joined the game, {}!\n", player.name))
                    .await
                    .unwrap();
                println!("{} {}, joined the game", player.name, player.id);

                game.add_player(&player.name, &player.id);
                let broadcast = broadcast.clone();
                drop(channels);
                broadcast(
                    format!("Testing the brocast. {} joined the game\n", player.name,),
                    player.id.clone(),
                )
                .await;
            }
            drop(lobbyplayers);
            let mut game = game.clone();
            let current_player = current_player_clone.clone();
            //give the dealer a card
            if let Some(card) = game.deck.deal_card() {
                game.dealer.cards.push(card);
            } else {
                //if the deck is empty, shuffle and deal a card
                game.deck = Deck::new();
                game.dealer.cards.push(game.deck.deal_card().unwrap());
            }
            for player in game.player_pool.iter_mut() {
                let mut current_player = current_player.lock().await;
                *current_player = Some(player.id.clone());
                drop(current_player);
                game.in_progress = true;
                //loop through all the players and deal them cards
                let channels = player_channels.lock().await;
                //send all the other players a mesage saying they need to wait for the current player to bet
                let broadcast_clone = broadcast.clone();
                drop(channels);
                broadcast_clone(format!("{} is betting\n", &player.name), player.id.clone()).await;
                let channels = player_channels.lock().await;
                let tx = channels.get(&player.id).unwrap();

                //present the player with the dealer's cards
                send_tx(&display_cards(&game.dealer), &tx);
                send_tx(&format!("Ok, {} it's your turn\n", player.name), tx);
                //deal the player two cards
                if let Some(card) = game.deck.deal_card() {
                    player.cards.push(card);
                } else {
                    //if there are no more cards in the deck reshuffle
                    game.deck = Deck::new();
                    game.deck.shuffle();
                    player.cards.push(game.deck.deal_card().unwrap());
                }
                //show the player their cards
                send_tx(&display_cards(player), tx);
                drop(channels);
                //continusly loop until player has placed a bet sleeping in order to yield the thread
                let player_bet: u32;
                loop {
                    let player_bet_pool = player_bet_pool_clone.lock().await;
                    if player_bet_pool.contains_key(&player.id) {
                        player_bet = *player_bet_pool.get(&player.id).unwrap();
                        break;
                    }
                    //remove the amount of money from the player
                    drop(player_bet_pool);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                player.money -= player_bet;
                let channels = player_channels.lock().await;
                let tx = channels.get(&player.id).unwrap();
                let player_bet_pool = player_bet_pool_clone.lock().await;
                tx.send(format!("You have bet ${}\n", &player_bet))
                    .await
                    .unwrap();
                let broadcast_clone = broadcast.clone();
                tx.send(format!("You have ${} left\n", player.money))
                    .await
                    .unwrap();
                drop(channels);
                broadcast_clone(
                    format!(
                        "{} has bet ${}\n",
                        player.name,
                        player_bet_pool.get(&player.id).unwrap()
                    ),
                    player.id.clone(),
                )
                .await;
                player.money -= player_bet;
            }
            //reset the player bet pool for the next round
            reset_game(player_bet_pool_clone.clone(), &mut game).await;
        }
    });

    let listener = TcpListener::bind("localhost:8080").await.unwrap();
    let player_bet_pool_clone = player_bet_pool.clone();
    let current_player_clone = current_player.clone();
    loop {
        let (mut stream, id) = listener.accept().await.unwrap();
        let lobby = Arc::clone(&player_lobby_clone);
        let player_channels_clone = Arc::clone(&player_channels_clone);
        let player_bet_pool_clone = Arc::clone(&player_bet_pool_clone);
        let current_player = Arc::clone(&current_player_clone);
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
            let (tx, mut rx) = mpsc::channel::<String>(32);
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
                        let mut player_bet_pool = player_bet_pool_clone.lock().await;
                        if result == 0 {
                            println!("{} disconnected", id);
                            break;
                        }
                        if let Ok(bet) = line.trim().parse::<u32>() {
                            //need to check if the current player is the one who is betting
                            let current_player = current_player.lock().await;
                            if let Some(current_player_id) = &*current_player {
                                println!("current player : {}", current_player_id);
                                if current_player_id == &id.to_string() {
                                    drop(current_player);
                                    player_bet_pool.insert(id.to_string(), bet);
                                    drop(player_bet_pool);
                                    // println!("{} bet {}", id, bet);
                                } else {
                                    println!("{} tried to bet but it's not their turn", id);
                                }
                            }
                        }
                        line.clear();
                    }
                }
            }
            //add the player to the lobby
        });
    }
}
