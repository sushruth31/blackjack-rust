//! The line protocol spoken over the socket: text in, text out.
//!
//! Everything a client sees is produced here, so [`crate::table`] never has to
//! know that it is being played over a network.

use crate::card::Card;
use crate::hand::Total;
use crate::rules::Outcome;
use crate::table::{Command, Event};

pub const GREETING: &str = "Welcome to the table. What is your name?\n";
pub const HELP: &str = "Commands: bet <chips> | hit | stand. Disconnect to leave.\n";

/// Parses one line of client input. Anything unrecognised is `None` and gets
/// a usage hint rather than being silently swallowed.
pub fn parse_command(line: &str) -> Option<Command> {
    let lowered = line.trim().to_lowercase();
    match lowered.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["hit"] | ["h"] => Some(Command::Hit),
        ["stand"] | ["s"] => Some(Command::Stand),
        ["bet", chips] | [chips] => chips.parse().ok().map(Command::Bet),
        _ => None,
    }
}

/// Blackjack is an open-handed game: every card except the dealer's hole card
/// is face up, so every event is broadcast to the whole table verbatim.
pub fn render(event: &Event) -> String {
    match event {
        Event::Joined { name, bankroll } => format!("{} sits down with {} chips\n", name, bankroll),
        Event::Left { name } => format!("{} leaves the table\n", name),
        Event::BettingOpened { min_bet } => format!("Place your bets (minimum {})\n", min_bet),
        Event::BetPlaced { name, amount } => format!("{} bets {}\n", name, amount),
        Event::DealerShows { card } => format!("Dealer shows {} and one down\n", card),
        Event::TurnStarted { name } => format!("{} to act -- hit or stand?\n", name),
        Event::Dealt { name, cards, total } => hand_line(name, cards, *total),
        Event::Bust { name, total } => format!("{} busts with {}\n", name, total.value),
        Event::Stood { name, total } => format!("{} stands on {}\n", name, score(*total)),
        Event::DealerHand { cards, total } => hand_line("Dealer", cards, *total),
        Event::OutOfChips { name } => format!("{} is out of chips and leaves\n", name),
        Event::Settled { name, outcome, payout, bankroll } => {
            format!("{} {} -- {} chips\n", name, settlement(*outcome, *payout), bankroll)
        }
    }
}

fn hand_line(who: &str, cards: &[Card], total: Total) -> String {
    let spread = cards.iter().map(Card::to_string).collect::<Vec<_>>().join(" ");
    format!("{}: {} ({})\n", who, spread, score(total))
}

fn score(total: Total) -> String {
    if total.soft {
        return format!("soft {}", total.value);
    }
    total.value.to_string()
}

fn settlement(outcome: Outcome, payout: u32) -> String {
    match outcome {
        Outcome::Blackjack => format!("has blackjack and collects {}", payout),
        Outcome::Win => format!("wins {}", payout),
        Outcome::Push => "pushes".to_string(),
        Outcome::Lose => "loses".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Rank, Suit};

    fn card(rank: Rank) -> Card {
        Card { rank, suit: Suit::Hearts }
    }

    #[test]
    fn hit_and_stand_accept_their_one_letter_forms() {
        assert_eq!(parse_command("hit"), Some(Command::Hit));
        assert_eq!(parse_command(" H \n"), Some(Command::Hit));
        assert_eq!(parse_command("STAND"), Some(Command::Stand));
        assert_eq!(parse_command("s"), Some(Command::Stand));
    }

    #[test]
    fn a_bet_may_be_written_with_or_without_the_verb() {
        assert_eq!(parse_command("bet 25"), Some(Command::Bet(25)));
        assert_eq!(parse_command("  25 "), Some(Command::Bet(25)));
    }

    #[test]
    fn nonsense_and_negative_wagers_are_rejected_rather_than_coerced() {
        assert_eq!(parse_command(""), None);
        assert_eq!(parse_command("fold"), None);
        assert_eq!(parse_command("bet -5"), None);
        assert_eq!(parse_command("bet 5 5"), None);
    }

    #[test]
    fn a_soft_total_is_labelled_soft_and_a_hard_one_is_not() {
        assert_eq!(score(Total { value: 17, soft: true }), "soft 17");
        assert_eq!(score(Total { value: 17, soft: false }), "17");
    }

    #[test]
    fn a_dealt_hand_renders_cards_then_total() {
        let event = Event::Dealt {
            name: "Ada".into(),
            cards: vec![card(Rank::Ace), card(Rank::King)],
            total: Total { value: 21, soft: true },
        };
        assert_eq!(render(&event), "Ada: A\u{2665} K\u{2665} (soft 21)\n");
    }

    #[test]
    fn every_event_renders_a_non_empty_line() {
        let total = Total { value: 20, soft: false };
        let events = [
            Event::Joined { name: "Ada".into(), bankroll: 100 },
            Event::Left { name: "Ada".into() },
            Event::BettingOpened { min_bet: 5 },
            Event::BetPlaced { name: "Ada".into(), amount: 5 },
            Event::DealerShows { card: card(Rank::Nine) },
            Event::TurnStarted { name: "Ada".into() },
            Event::Bust { name: "Ada".into(), total },
            Event::Stood { name: "Ada".into(), total },
            Event::DealerHand { cards: vec![card(Rank::Ten)], total },
            Event::Settled { name: "Ada".into(), outcome: Outcome::Push, payout: 5, bankroll: 100 },
            Event::OutOfChips { name: "Ada".into() },
        ];
        assert!(events.iter().all(|event| render(event).ends_with('\n')));
        assert!(events.iter().all(|event| render(event).len() > 1));
    }
}
