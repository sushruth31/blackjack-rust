//! End-to-end rounds driven through the public table API with a seeded shoe,
//! so the assertions below hold for real deals rather than hand-built hands.

use blackjack_rust::rules::Rules;
use blackjack_rust::table::{Command, Event, Phase, Table};
use rand::rngs::StdRng;
use rand::SeedableRng;

const HOUSE: Rules = Rules {
    packs: 6,
    dealer_hits_soft_17: false,
    min_bet: 5,
    starting_bankroll: 200,
    min_players: 1,
};

/// Seats one player, bets, and stands until the round settles.
fn stand_pat(seed: u64) -> (Table<StdRng>, Vec<Event>) {
    let mut table = Table::new(HOUSE, StdRng::seed_from_u64(seed));
    let mut log = table.join("p1", "Ada").expect("the table is empty");
    log.extend(table.apply("p1", Command::Bet(20)).expect("the bet is legal"));
    while table.phase() == Phase::PlayerTurns {
        log.extend(table.apply("p1", Command::Stand).expect("it is our turn"));
    }
    (table, log)
}

fn position(log: &[Event], wanted: fn(&Event) -> bool) -> Option<usize> {
    log.iter().position(wanted)
}

fn dealer_total(log: &[Event]) -> u16 {
    log.iter()
        .find_map(|event| match event {
            Event::DealerHand { total, .. } => Some(total.value),
            _ => None,
        })
        .expect("the dealer's hand is always revealed")
}

#[test]
fn a_round_runs_join_bet_deal_act_settle_and_then_reopens_betting() {
    let (table, log) = stand_pat(11);
    let dealt = position(&log, |e| matches!(e, Event::DealerShows { .. })).expect("cards are out");
    let settled = position(&log, |e| matches!(e, Event::Settled { .. })).expect("chips move");
    assert!(matches!(log.first(), Some(Event::Joined { .. })));
    assert!(dealt < settled, "the deal precedes settlement");
    assert_eq!(table.phase(), Phase::Betting, "the next round is open");
}

#[test]
fn the_dealer_never_stops_below_seventeen_while_a_live_hand_is_out() {
    for seed in 0..300 {
        let (_, log) = stand_pat(seed);
        let contested = position(&log, |e| matches!(e, Event::Stood { .. })).is_some();
        let total = dealer_total(&log);
        assert!(!contested || total >= 17, "seed {} left the dealer on {}", seed, total);
    }
}

#[test]
fn settlement_always_matches_the_published_bankroll() {
    for seed in 0..300 {
        let (table, log) = stand_pat(seed);
        let closing = log.iter().rev().find_map(|event| match event {
            Event::Settled { bankroll, .. } => Some(*bankroll),
            _ => None,
        });
        assert_eq!(closing, table.bankroll("p1"), "seed {}", seed);
    }
}

#[test]
fn a_wager_of_twenty_can_only_move_the_bankroll_to_one_of_four_amounts() {
    for seed in 0..300 {
        let (table, _) = stand_pat(seed);
        let bankroll = table.bankroll("p1").expect("the player still has a seat");
        assert!(
            [180, 200, 220, 230].contains(&bankroll),
            "seed {} produced an impossible bankroll {}",
            seed,
            bankroll
        );
    }
}
