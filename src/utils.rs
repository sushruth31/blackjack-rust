use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use rand::Rng;
#[derive(Clone, Debug)]
pub struct Card {
    pub suit: String,
    pub value: i32,
}
pub fn format_card(card: &Card) -> String {
    let mut card_str = String::new();
    match card.value {
        11 => card_str.push_str("J"),
        12 => card_str.push_str("Q"),
        13 => card_str.push_str("K"),
        14 => card_str.push_str("A"),
        _ => card_str.push_str(&card.value.to_string()),
    }
    card_str.push_str(&card.suit);
    return card_str;
}

#[derive(Clone, Debug)]
pub struct Player {
    pub name: String,
    pub money: u32,
    pub current_bet: u32,
    pub id: String,
    pub cards: Vec<Card>,
}

pub trait DisplayCards {
    fn display_cards(&self) -> String;
}

pub fn display_cards(obj: &impl DisplayCards) -> String {
    obj.display_cards()
}

impl Player {
    pub fn new(name: &str, id: &str) -> Self {
        Self {
            name: name.to_string(),
            money: 100,
            current_bet: 0,
            id: id.to_string(),
            cards: Vec::new(),
        }
    }
}

impl DisplayCards for Player {
    fn display_cards(&self) -> String {
        let mut target = "Your cards: ".to_string();
        self.cards.iter().for_each(|card| {
            let Card { suit, value } = card;
            target.push_str(&format!("{} of {}, ", value, suit));
        });
        return target;
    }
}

impl DisplayCards for Dealer {
    fn display_cards(&self) -> String {
        let mut target = "Dealer's Cards: ".to_string();
        self.cards.iter().for_each(|card| {
            let Card { suit, value } = card;
            target.push_str(&format!("{} of {}, ", value, suit));
        });
        return target;
    }
}

#[derive(Clone, Debug)]
pub struct Dealer {
    pub cards: Vec<Card>,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub player_pool: Vec<Player>,
    pub deck: Deck,
    pub dealer: Dealer,
    pub in_progress: bool,
}

impl Game {
    pub fn add_player(&mut self, name: &str, id: &str) {
        self.player_pool.push(Player {
            name: name.to_string(),
            money: 100,
            current_bet: 0,
            id: id.to_string(),
            cards: Vec::new(),
        });
    }
}

#[derive(Clone, Debug)]
pub struct Deck(pub Vec<Card>);

impl Deck {
    pub fn new() -> Self {
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
        Self(deck.to_vec()).shuffle();
        return Self(deck);
    }
    pub fn shuffle(&mut self) {
        let cards = &mut self.0;
        for i in 0..cards.len() {
            let rand = rand::thread_rng().gen_range(0..cards.len());
            cards.swap(i, rand);
        }
    }

    pub fn deal_card(&mut self) -> Option<Card> {
        return self.0.pop();
    }
}

pub async fn reset_game(player_bets: Arc<Mutex<HashMap<String, u32>>>, game: &mut Game) {
    //reset the player bet pool for the next round
    let mut player_bet_pool = player_bets.lock().await;
    player_bet_pool.clear();
    //clear out everyones cards including the dealer
    for player in game.player_pool.iter_mut() {
        player.cards.clear();
    }
    game.dealer.cards.clear();
}
