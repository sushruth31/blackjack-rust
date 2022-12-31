//! The round state machine. Pure: commands in, events out, no I/O.

use crate::card::{Card, Deck};
use crate::hand::{Hand, Total};
use crate::rules::{self, Outcome, Rules};
use rand::Rng;
use std::error::Error;
use std::fmt;

pub const MAX_SEATS: usize = 7;

pub type SeatId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    WaitingForPlayers,
    Betting,
    PlayerTurns,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Bet(u32),
    Hit,
    Stand,
}

/// What happened, not how to say it. Rendering lives in [`crate::protocol`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Joined { name: String, bankroll: u32 },
    Left { name: String },
    BettingOpened { min_bet: u32 },
    BetPlaced { name: String, amount: u32 },
    Dealt { name: String, cards: Vec<Card>, total: Total },
    DealerShows { card: Card },
    TurnStarted { name: String },
    Bust { name: String, total: Total },
    Stood { name: String, total: Total },
    DealerHand { cards: Vec<Card>, total: Total },
    Settled { name: String, outcome: Outcome, payout: u32, bankroll: u32 },
    OutOfChips { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    UnknownSeat,
    AlreadySeated,
    TableFull,
    WrongPhase,
    AlreadyBet,
    NotYourTurn,
    BetTooSmall { min_bet: u32 },
    InsufficientChips { bankroll: u32 },
    ShoeExhausted,
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSeat => write!(f, "you are not seated at this table"),
            Self::AlreadySeated => write!(f, "you already hold a seat"),
            Self::TableFull => write!(f, "the table is full ({} seats)", MAX_SEATS),
            Self::WrongPhase => write!(f, "that is not a legal move right now"),
            Self::AlreadyBet => write!(f, "your bet is already down for this round"),
            Self::NotYourTurn => write!(f, "wait your turn"),
            Self::BetTooSmall { min_bet } => write!(f, "the minimum bet is {}", min_bet),
            Self::InsufficientChips { bankroll } => write!(f, "you only have {}", bankroll),
            Self::ShoeExhausted => write!(f, "the shoe ran out"),
        }
    }
}

impl Error for TableError {}

#[derive(Clone, Debug)]
struct Seat {
    id: SeatId,
    name: String,
    bankroll: u32,
    bet: u32,
    hand: Hand,
    done: bool,
}

impl Seat {
    fn new(id: &str, name: &str, bankroll: u32, done: bool) -> Self {
        Seat {
            id: id.to_string(),
            name: name.to_string(),
            bankroll,
            bet: 0,
            hand: Hand::default(),
            done,
        }
    }

    fn dealt_event(&self) -> Event {
        Event::Dealt {
            name: self.name.clone(),
            cards: self.hand.cards().to_vec(),
            total: self.hand.total(),
        }
    }

    fn clear(&mut self) {
        self.hand.clear();
        self.bet = 0;
        self.done = false;
    }
}

/// One blackjack table. Generic over the RNG so tests can seed it and get a
/// deterministic shoe.
pub struct Table<R: Rng> {
    rules: Rules,
    seats: Vec<Seat>,
    dealer: Hand,
    deck: Deck,
    phase: Phase,
    turn: Option<SeatId>,
    rng: R,
}

impl<R: Rng> Table<R> {
    pub fn new(rules: Rules, mut rng: R) -> Self {
        let mut deck = Deck::new(rules.packs);
        deck.shuffle(&mut rng);
        Table {
            rules,
            seats: Vec::new(),
            dealer: Hand::default(),
            deck,
            phase: Phase::WaitingForPlayers,
            turn: None,
            rng,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn bankroll(&self, id: &str) -> Option<u32> {
        self.seats.iter().find(|seat| seat.id == id).map(|seat| seat.bankroll)
    }

    /// Seats a player. Anyone arriving mid-round sits out until the next deal.
    pub fn join(&mut self, id: &str, name: &str) -> Result<Vec<Event>, TableError> {
        if self.seats.iter().any(|seat| seat.id == id) {
            return Err(TableError::AlreadySeated);
        }
        if self.seats.len() >= MAX_SEATS {
            return Err(TableError::TableFull);
        }
        let sitting_out = self.phase == Phase::PlayerTurns;
        let bankroll = self.rules.starting_bankroll;
        self.seats.push(Seat::new(id, name, bankroll, sitting_out));
        let mut events = vec![Event::Joined { name: name.to_string(), bankroll }];
        self.open_betting_if_ready(&mut events);
        Ok(events)
    }

    /// Removes a player. A wager already on the felt is forfeited, and if the
    /// table was waiting on them the round moves on rather than deadlocking.
    pub fn leave(&mut self, id: &str) -> Vec<Event> {
        let Some(index) = self.seats.iter().position(|seat| seat.id == id) else {
            return Vec::new();
        };
        let mut events = vec![Event::Left { name: self.seats.remove(index).name }];
        if self.turn.as_deref() == Some(id) {
            let _ = self.advance_turn(&mut events);
        }
        if self.seats.is_empty() {
            self.abandon_round();
        }
        events
    }

    pub fn apply(&mut self, id: &str, command: Command) -> Result<Vec<Event>, TableError> {
        match command {
            Command::Bet(amount) => self.bet(id, amount),
            Command::Hit => self.hit(id),
            Command::Stand => self.stand(id),
        }
    }

    fn bet(&mut self, id: &str, amount: u32) -> Result<Vec<Event>, TableError> {
        if self.phase != Phase::Betting {
            return Err(TableError::WrongPhase);
        }
        let min_bet = self.rules.min_bet;
        let seat = self.seat_mut(id)?;
        check_wager(seat, amount, min_bet)?;
        seat.bankroll -= amount;
        seat.bet = amount;
        let mut events = vec![Event::BetPlaced { name: seat.name.clone(), amount }];
        if self.seats.iter().all(|seat| seat.bet > 0) {
            self.deal_round(&mut events)?;
        }
        Ok(events)
    }

    fn hit(&mut self, id: &str) -> Result<Vec<Event>, TableError> {
        self.require_turn(id)?;
        let card = self.draw()?;
        let seat = self.seat_mut(id)?;
        seat.hand.push(card);
        let (name, total) = (seat.name.clone(), seat.hand.total());
        let mut events = vec![seat.dealt_event()];
        if total.value < 21 {
            return Ok(events);
        }
        seat.done = true;
        events.push(hand_closed(name, total));
        self.advance_turn(&mut events)?;
        Ok(events)
    }

    fn stand(&mut self, id: &str) -> Result<Vec<Event>, TableError> {
        self.require_turn(id)?;
        let seat = self.seat_mut(id)?;
        seat.done = true;
        let mut events = vec![Event::Stood { name: seat.name.clone(), total: seat.hand.total() }];
        self.advance_turn(&mut events)?;
        Ok(events)
    }

    fn require_turn(&self, id: &str) -> Result<(), TableError> {
        if self.phase != Phase::PlayerTurns {
            return Err(TableError::WrongPhase);
        }
        if self.turn.as_deref() != Some(id) {
            return Err(TableError::NotYourTurn);
        }
        Ok(())
    }

    fn seat_mut(&mut self, id: &str) -> Result<&mut Seat, TableError> {
        self.seats.iter_mut().find(|seat| seat.id == id).ok_or(TableError::UnknownSeat)
    }

    fn open_betting_if_ready(&mut self, events: &mut Vec<Event>) {
        if self.phase != Phase::WaitingForPlayers || self.seats.len() < self.rules.min_players {
            return;
        }
        self.phase = Phase::Betting;
        events.push(Event::BettingOpened { min_bet: self.rules.min_bet });
    }

    fn deal_round(&mut self, events: &mut Vec<Event>) -> Result<(), TableError> {
        self.replenish_at_cut_card();
        self.deal_opening_cards()?;
        self.phase = Phase::PlayerTurns;
        events.extend(self.seats.iter().map(Seat::dealt_event));
        if let Some(&card) = self.dealer.cards().first() {
            events.push(Event::DealerShows { card });
        }
        for seat in self.seats.iter_mut() {
            seat.done = seat.hand.is_blackjack();
        }
        self.advance_turn(events)
    }

    /// Two passes round the table, dealer last, exactly as it is dealt live.
    fn deal_opening_cards(&mut self) -> Result<(), TableError> {
        for _ in 0..2 {
            for index in 0..self.seats.len() {
                let card = self.draw()?;
                self.seats[index].hand.push(card);
            }
            let card = self.draw()?;
            self.dealer.push(card);
        }
        Ok(())
    }

    fn advance_turn(&mut self, events: &mut Vec<Event>) -> Result<(), TableError> {
        let next = self.seats.iter().find(|seat| !seat.done);
        let Some((id, name)) = next.map(|seat| (seat.id.clone(), seat.name.clone())) else {
            return self.finish_round(events);
        };
        self.turn = Some(id);
        events.push(Event::TurnStarted { name });
        Ok(())
    }

    fn finish_round(&mut self, events: &mut Vec<Event>) -> Result<(), TableError> {
        self.turn = None;
        self.play_dealer(events)?;
        self.settle(events);
        self.reset_for_next_round(events);
        Ok(())
    }

    /// The dealer stands pat when every live hand is already bust or a
    /// natural: neither result can change, since a natural is decided by the
    /// two cards the dealer already holds.
    fn play_dealer(&mut self, events: &mut Vec<Event>) -> Result<(), TableError> {
        while self.dealer_must_draw() {
            let card = self.draw()?;
            self.dealer.push(card);
        }
        events.push(Event::DealerHand {
            cards: self.dealer.cards().to_vec(),
            total: self.dealer.total(),
        });
        Ok(())
    }

    fn dealer_must_draw(&self) -> bool {
        let decided = self.wagered().all(|seat| seat.hand.is_bust() || seat.hand.is_blackjack());
        !decided && rules::dealer_should_hit(&self.dealer, self.rules.dealer_hits_soft_17)
    }

    fn wagered(&self) -> impl Iterator<Item = &Seat> + '_ {
        self.seats.iter().filter(|seat| seat.bet > 0)
    }

    fn settle(&mut self, events: &mut Vec<Event>) {
        let dealer = self.dealer.clone();
        let settled = self.seats.iter_mut().filter(|seat| seat.bet > 0);
        events.extend(settled.map(|seat| settle_seat(seat, &dealer)));
    }

    fn reset_for_next_round(&mut self, events: &mut Vec<Event>) {
        self.dealer.clear();
        self.seats.iter_mut().for_each(Seat::clear);
        let min_bet = self.rules.min_bet;
        let broke = self.seats.iter().filter(|seat| seat.bankroll < min_bet);
        events.extend(broke.map(|seat| Event::OutOfChips { name: seat.name.clone() }));
        self.seats.retain(|seat| seat.bankroll >= min_bet);
        self.phase = Phase::WaitingForPlayers;
        self.open_betting_if_ready(events);
    }

    fn abandon_round(&mut self) {
        self.dealer.clear();
        self.turn = None;
        self.phase = Phase::WaitingForPlayers;
    }

    fn draw(&mut self) -> Result<Card, TableError> {
        if self.deck.is_empty() {
            self.replenish();
        }
        self.deck.draw().ok_or(TableError::ShoeExhausted)
    }

    /// Cut card at 25% penetration, checked between rounds so a shuffle never
    /// lands in the middle of a hand.
    fn replenish_at_cut_card(&mut self) {
        if self.deck.len() < 13 * self.rules.packs as usize {
            self.replenish();
        }
    }

    fn replenish(&mut self) {
        self.deck = Deck::new(self.rules.packs);
        self.deck.shuffle(&mut self.rng);
    }
}

fn check_wager(seat: &Seat, amount: u32, min_bet: u32) -> Result<(), TableError> {
    if seat.bet > 0 {
        return Err(TableError::AlreadyBet);
    }
    if amount < min_bet {
        return Err(TableError::BetTooSmall { min_bet });
    }
    if amount > seat.bankroll {
        return Err(TableError::InsufficientChips { bankroll: seat.bankroll });
    }
    Ok(())
}

fn hand_closed(name: String, total: Total) -> Event {
    if total.value > 21 {
        Event::Bust { name, total }
    } else {
        Event::Stood { name, total }
    }
}

fn settle_seat(seat: &mut Seat, dealer: &Hand) -> Event {
    let outcome = rules::resolve(&seat.hand, dealer);
    let payout = rules::payout(outcome, seat.bet);
    seat.bankroll += payout;
    Event::Settled { name: seat.name.clone(), outcome, payout, bankroll: seat.bankroll }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn rules() -> Rules {
        Rules {
            packs: 6,
            dealer_hits_soft_17: false,
            min_bet: 5,
            starting_bankroll: 100,
            min_players: 1,
        }
    }

    fn table(rules: Rules) -> Table<StdRng> {
        Table::new(rules, StdRng::seed_from_u64(2022))
    }

    fn seated(rules: Rules, ids: &[&str]) -> Table<StdRng> {
        let mut table = table(rules);
        for id in ids {
            table.join(id, id).expect("seat is free");
        }
        table
    }

    fn play_to_settlement(table: &mut Table<StdRng>, id: &str) -> Vec<Event> {
        let mut events = table.apply(id, Command::Bet(10)).expect("bet accepted");
        while table.phase() == Phase::PlayerTurns {
            events.extend(table.apply(id, Command::Stand).expect("stand accepted"));
        }
        events
    }

    fn settled_outcome(events: &[Event]) -> Option<Outcome> {
        events.iter().find_map(|event| match event {
            Event::Settled { outcome, .. } => Some(*outcome),
            _ => None,
        })
    }

    #[test]
    fn table_waits_for_the_minimum_player_count_before_betting_opens() {
        let mut table = table(Rules { min_players: 2, ..rules() });
        table.join("a", "Ada").expect("seat is free");
        assert_eq!(table.phase(), Phase::WaitingForPlayers);
        table.join("b", "Bob").expect("seat is free");
        assert_eq!(table.phase(), Phase::Betting);
    }

    #[test]
    fn the_same_player_cannot_take_two_seats() {
        let mut table = seated(rules(), &["a"]);
        assert_eq!(table.join("a", "Ada"), Err(TableError::AlreadySeated));
    }

    #[test]
    fn the_table_holds_at_most_seven_seats() {
        let ids: Vec<String> = (0..MAX_SEATS).map(|n| n.to_string()).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let mut table = seated(rules(), &refs);
        assert_eq!(table.join("late", "Late"), Err(TableError::TableFull));
    }

    #[test]
    fn a_bet_below_the_minimum_is_rejected_and_leaves_the_bankroll_alone() {
        let mut table = seated(rules(), &["a"]);
        let error = table.apply("a", Command::Bet(1)).unwrap_err();
        assert_eq!(error, TableError::BetTooSmall { min_bet: 5 });
        assert_eq!(table.bankroll("a"), Some(100));
    }

    #[test]
    fn a_bet_larger_than_the_bankroll_is_rejected() {
        let mut table = seated(rules(), &["a"]);
        let error = table.apply("a", Command::Bet(101)).unwrap_err();
        assert_eq!(error, TableError::InsufficientChips { bankroll: 100 });
    }

    #[test]
    fn a_wager_is_taken_from_the_bankroll_exactly_once() {
        let mut table = seated(rules(), &["a", "b"]);
        table.apply("a", Command::Bet(30)).expect("bet accepted");
        assert_eq!(table.bankroll("a"), Some(70));
        assert_eq!(table.apply("a", Command::Bet(30)), Err(TableError::AlreadyBet));
        assert_eq!(table.bankroll("a"), Some(70));
    }

    #[test]
    fn cards_are_dealt_only_once_every_seat_has_wagered() {
        let mut table = seated(rules(), &["a", "b"]);
        table.apply("a", Command::Bet(5)).expect("bet accepted");
        assert_eq!(table.phase(), Phase::Betting);
        table.apply("b", Command::Bet(5)).expect("bet accepted");
        assert_eq!(table.phase(), Phase::PlayerTurns);
    }

    #[test]
    fn acting_out_of_turn_is_rejected() {
        let mut table = seated(rules(), &["a", "b"]);
        table.apply("a", Command::Bet(5)).expect("bet accepted");
        table.apply("b", Command::Bet(5)).expect("bet accepted");
        assert_eq!(table.apply("b", Command::Hit), Err(TableError::NotYourTurn));
    }

    #[test]
    fn hitting_before_the_cards_are_out_is_rejected() {
        let mut table = seated(rules(), &["a"]);
        assert_eq!(table.apply("a", Command::Hit), Err(TableError::WrongPhase));
    }

    #[test]
    fn a_round_always_settles_every_wagered_seat_and_reopens_betting() {
        let mut table = seated(rules(), &["a"]);
        let events = play_to_settlement(&mut table, "a");
        assert!(settled_outcome(&events).is_some());
        assert_eq!(table.phase(), Phase::Betting);
    }

    #[test]
    fn the_bankroll_moves_by_the_wager_in_the_direction_of_the_outcome() {
        let mut table = seated(rules(), &["a"]);
        let events = play_to_settlement(&mut table, "a");
        let expected = match settled_outcome(&events).expect("settled") {
            Outcome::Blackjack => 115,
            Outcome::Win => 110,
            Outcome::Push => 100,
            Outcome::Lose => 90,
        };
        assert_eq!(table.bankroll("a"), Some(expected));
    }

    #[test]
    fn five_hundred_rounds_always_reach_settlement() {
        let mut table = seated(rules(), &["a", "b"]);
        for _ in 0..500 {
            if table.phase() != Phase::Betting {
                break;
            }
            drain_round(&mut table);
            assert_ne!(table.phase(), Phase::PlayerTurns, "round must settle");
        }
    }

    fn drain_round(table: &mut Table<StdRng>) {
        ["a", "b"].into_iter().for_each(|id| ignore(table.apply(id, Command::Bet(5))));
        while table.phase() == Phase::PlayerTurns {
            ["a", "b"].into_iter().for_each(|id| ignore(table.apply(id, Command::Stand)));
        }
    }

    /// Seats that are not up, or have already left, legitimately refuse.
    fn ignore(result: Result<Vec<Event>, TableError>) {
        let _ = result;
    }

    #[test]
    fn leaving_on_your_turn_hands_the_shoe_to_the_next_seat() {
        let mut table = seated(rules(), &["a", "b"]);
        table.apply("a", Command::Bet(5)).expect("bet accepted");
        table.apply("b", Command::Bet(5)).expect("bet accepted");
        table.leave("a");
        assert!(table.bankroll("a").is_none());
        assert!(table.apply("b", Command::Stand).is_ok());
    }

    #[test]
    fn the_last_player_leaving_returns_the_table_to_waiting() {
        let mut table = seated(rules(), &["a"]);
        table.apply("a", Command::Bet(5)).expect("bet accepted");
        table.leave("a");
        assert_eq!(table.phase(), Phase::WaitingForPlayers);
    }

    #[test]
    fn leaving_a_seat_that_does_not_exist_is_a_no_op() {
        let mut table = seated(rules(), &["a"]);
        assert!(table.leave("ghost").is_empty());
    }

    #[test]
    fn a_player_who_cannot_cover_the_minimum_is_dropped_after_settlement() {
        let broke = Rules { min_bet: 100, starting_bankroll: 100, ..rules() };
        let mut table = seated(broke, &["a"]);
        let mut events = table.apply("a", Command::Bet(100)).expect("bet accepted");
        while table.phase() == Phase::PlayerTurns {
            events.extend(table.apply("a", Command::Stand).expect("stand accepted"));
        }
        let lost = settled_outcome(&events) == Some(Outcome::Lose);
        assert_eq!(table.bankroll("a").is_none(), lost);
    }

    #[test]
    fn the_shoe_is_replenished_rather_than_running_dry() {
        let mut table = seated(Rules { packs: 1, ..rules() }, &["a"]);
        for _ in 0..200 {
            let mut events = match table.apply("a", Command::Bet(5)) {
                Ok(events) => events,
                Err(_) => break,
            };
            while table.phase() == Phase::PlayerTurns {
                events.extend(table.apply("a", Command::Hit).expect("hit accepted"));
            }
            assert!(settled_outcome(&events).is_some());
        }
    }
}
