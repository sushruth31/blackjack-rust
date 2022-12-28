//! Table rules: the dealer's drawing policy, hand comparison, and payouts.

use crate::hand::Hand;

/// Everything about how a table plays, independent of how it is served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub packs: u8,
    pub dealer_hits_soft_17: bool,
    pub min_bet: u32,
    pub starting_bankroll: u32,
    pub min_players: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Blackjack,
    Win,
    Push,
    Lose,
}

/// The dealer has no choices: draw below 17, stand at 17 or better. The only
/// house variation is soft 17 (H17 costs the player roughly 0.2% of edge),
/// so it is the one knob exposed here.
pub fn dealer_should_hit(dealer: &Hand, hits_soft_17: bool) -> bool {
    let total = dealer.total();
    total.value < 17 || (hits_soft_17 && total.soft && total.value == 17)
}

/// Compare a settled player hand against the dealer's final hand.
pub fn resolve(player: &Hand, dealer: &Hand) -> Outcome {
    if player.is_bust() {
        return Outcome::Lose;
    }
    if let Some(outcome) = resolve_naturals(player, dealer) {
        return outcome;
    }
    if dealer.is_bust() {
        return Outcome::Win;
    }
    compare_totals(player, dealer)
}

/// A player bust loses even when the dealer also busts — the player acts
/// first, and that ordering is the house edge. Naturals settle before any
/// total comparison, so 21 on two cards beats a dealer's drawn 21.
fn resolve_naturals(player: &Hand, dealer: &Hand) -> Option<Outcome> {
    match (player.is_blackjack(), dealer.is_blackjack()) {
        (true, true) => Some(Outcome::Push),
        (true, false) => Some(Outcome::Blackjack),
        (false, true) => Some(Outcome::Lose),
        (false, false) => None,
    }
}

fn compare_totals(player: &Hand, dealer: &Hand) -> Outcome {
    match player.total().value.cmp(&dealer.total().value) {
        std::cmp::Ordering::Greater => Outcome::Win,
        std::cmp::Ordering::Equal => Outcome::Push,
        std::cmp::Ordering::Less => Outcome::Lose,
    }
}

/// Chips returned to the player, wager included, given that the wager was
/// already taken off the bankroll when it was placed.
///
/// Chips are whole units, so a 3:2 natural on an odd wager rounds down in the
/// house's favour — the same reason casinos ask for even bets.
pub fn payout(outcome: Outcome, bet: u32) -> u32 {
    match outcome {
        Outcome::Blackjack => bet + bet * 3 / 2,
        Outcome::Win => bet * 2,
        Outcome::Push => bet,
        Outcome::Lose => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Rank::*;
    use crate::hand::fixtures::hand;

    #[test]
    fn dealer_draws_below_seventeen_and_stands_on_hard_seventeen() {
        assert!(dealer_should_hit(&hand(&[Ten, Six]), false));
        assert!(!dealer_should_hit(&hand(&[Ten, Seven]), false));
        assert!(!dealer_should_hit(&hand(&[Ten, Nine]), false));
    }

    #[test]
    fn dealer_stands_on_soft_seventeen_under_s17() {
        assert!(!dealer_should_hit(&hand(&[Ace, Six]), false));
    }

    #[test]
    fn dealer_hits_soft_seventeen_under_h17_but_not_soft_eighteen() {
        assert!(dealer_should_hit(&hand(&[Ace, Six]), true));
        assert!(!dealer_should_hit(&hand(&[Ace, Seven]), true));
    }

    #[test]
    fn h17_does_not_apply_to_a_hard_seventeen_containing_an_ace() {
        assert!(!dealer_should_hit(&hand(&[Ace, Six, Ten]), true));
        assert!(dealer_should_hit(&hand(&[Ace, Two, Three]), true));
    }

    #[test]
    fn player_bust_loses_even_when_the_dealer_also_busts() {
        let outcome = resolve(&hand(&[Ten, Nine, Five]), &hand(&[Ten, Six, Ten]));
        assert_eq!(outcome, Outcome::Lose);
    }

    #[test]
    fn dealer_bust_pays_any_standing_player() {
        assert_eq!(resolve(&hand(&[Five, Six]), &hand(&[Ten, Six, Ten])), Outcome::Win);
    }

    #[test]
    fn natural_beats_a_drawn_twenty_one() {
        let outcome = resolve(&hand(&[Ace, King]), &hand(&[Seven, Seven, Seven]));
        assert_eq!(outcome, Outcome::Blackjack);
    }

    #[test]
    fn two_naturals_push() {
        assert_eq!(resolve(&hand(&[Ace, King]), &hand(&[Ace, Queen])), Outcome::Push);
    }

    #[test]
    fn dealer_natural_beats_a_drawn_twenty_one() {
        let outcome = resolve(&hand(&[Seven, Seven, Seven]), &hand(&[Ace, Ten]));
        assert_eq!(outcome, Outcome::Lose);
    }

    #[test]
    fn equal_totals_push_and_higher_total_wins() {
        assert_eq!(resolve(&hand(&[Ten, Eight]), &hand(&[Nine, Nine])), Outcome::Push);
        assert_eq!(resolve(&hand(&[Ten, Nine]), &hand(&[Nine, Nine])), Outcome::Win);
        assert_eq!(resolve(&hand(&[Ten, Seven]), &hand(&[Nine, Nine])), Outcome::Lose);
    }

    #[test]
    fn a_soft_total_compares_at_its_high_value() {
        assert_eq!(resolve(&hand(&[Ace, Seven]), &hand(&[Ten, Seven])), Outcome::Win);
    }

    #[test]
    fn natural_pays_three_to_two_and_a_win_pays_even_money() {
        assert_eq!(payout(Outcome::Blackjack, 10), 25);
        assert_eq!(payout(Outcome::Win, 10), 20);
        assert_eq!(payout(Outcome::Push, 10), 10);
        assert_eq!(payout(Outcome::Lose, 10), 0);
    }

    #[test]
    fn natural_on_an_odd_wager_rounds_down() {
        assert_eq!(payout(Outcome::Blackjack, 5), 12);
        assert_eq!(payout(Outcome::Blackjack, 1), 2);
    }
}
